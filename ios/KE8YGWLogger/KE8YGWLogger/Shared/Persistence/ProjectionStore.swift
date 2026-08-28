import Foundation
import OSLog
import SwiftData
import SwiftUI

/// How the SwiftData projection cache was opened at launch.
///
/// SwiftData is a cache/projection of the Rust-owned event store, never the
/// authority, so a cache that cannot be opened is recoverable: the app moves it
/// aside and rebuilds from Rust instead of refusing to launch.
enum ProjectionStoreRecovery: Equatable {
    /// The on-disk cache opened normally.
    case opened
    /// The on-disk cache could not be opened. It was moved into quarantine and
    /// replaced with an empty cache that must be rebuilt from Rust.
    case rebuiltAfterFailure(reason: String, quarantine: URL?)
    /// Neither the existing nor a replacement on-disk cache could be opened, so
    /// this launch runs against an in-memory cache.
    case inMemoryFallback(reason: String)
    /// No cache of any kind could be created. The app cannot show logbook data.
    case unavailable(reason: String)

    /// True when the cache starts empty and has to be repopulated from Rust.
    var requiresFullProjectionRebuild: Bool {
        switch self {
        case .opened:
            return false
        case .rebuiltAfterFailure, .inMemoryFallback, .unavailable:
            return true
        }
    }

    var label: String {
        switch self {
        case .opened: return "healthy"
        case .rebuiltAfterFailure: return "rebuilt after failure"
        case .inMemoryFallback: return "in-memory fallback"
        case .unavailable: return "unavailable"
        }
    }

    var reason: String? {
        switch self {
        case .opened:
            return nil
        case .rebuiltAfterFailure(let reason, _),
             .inMemoryFallback(let reason),
             .unavailable(let reason):
            return reason
        }
    }

    var quarantineURL: URL? {
        switch self {
        case .rebuiltAfterFailure(_, let quarantine): return quarantine
        case .opened, .inMemoryFallback, .unavailable: return nil
        }
    }
}

/// A projection cache container plus the story of how it was obtained.
struct ProjectionStore {
    /// `nil` only when no container could be created at all, in which case the
    /// app shows a recovery screen rather than trapping.
    let container: ModelContainer?
    let recovery: ProjectionStoreRecovery
}

enum ProjectionStoreFactory {
    static let projectionModels: [any PersistentModel.Type] = [
        QSO.self,
        StationProfile.self,
        StationEquipment.self,
        AppSettings.self
    ]

    /// Quarantined stores live beside the active one so they can be pulled from
    /// a sysdiagnose, and are pruned so they cannot grow without bound.
    static let quarantineDirectoryName = "QuarantinedProjectionCache"
    static let maxQuarantinedStores = 3

    private static let logger = Logger(
        subsystem: "com.h2technologiesllc.ke8ygw-logger",
        category: "ProjectionStore"
    )

    static func make() -> ProjectionStore {
        let schema = Schema(projectionModels)
        return make(schema: schema, configuration: ModelConfiguration(schema: schema)) { schema, configuration in
            try ModelContainer(for: schema, configurations: configuration)
        }
    }

    /// Opens the cache, degrading rather than trapping at every step.
    ///
    /// `makeContainer` is injected so the recovery ladder can be exercised in
    /// tests without corrupting a real store.
    static func make(
        schema: Schema,
        configuration: ModelConfiguration,
        makeContainer: (Schema, ModelConfiguration) throws -> ModelContainer
    ) -> ProjectionStore {
        do {
            let container = try makeContainer(schema, configuration)
            return ProjectionStore(container: container, recovery: .opened)
        } catch {
            let reason = describe(error)
            logger.error("Projection cache failed to open: \(reason, privacy: .public)")

            let quarantine = quarantineStore(at: configuration.url)
            do {
                let container = try makeContainer(schema, configuration)
                logger.notice("Projection cache replaced; rebuilding the projection from Rust.")
                return ProjectionStore(
                    container: container,
                    recovery: .rebuiltAfterFailure(reason: reason, quarantine: quarantine)
                )
            } catch {
                logger.error("Projection cache could not be recreated: \(describe(error), privacy: .public)")
            }

            do {
                let memoryConfiguration = ModelConfiguration(schema: schema, isStoredInMemoryOnly: true)
                let container = try makeContainer(schema, memoryConfiguration)
                logger.notice("Projection cache running in memory for this launch.")
                return ProjectionStore(container: container, recovery: .inMemoryFallback(reason: reason))
            } catch {
                logger.fault("No projection cache could be created: \(describe(error), privacy: .public)")
                return ProjectionStore(container: nil, recovery: .unavailable(reason: reason))
            }
        }
    }

    /// Moves an unopenable cache into a timestamped quarantine folder.
    ///
    /// The files are moved rather than deleted. Nothing authoritative lives in
    /// the cache, but a store written before the Rust bridge became the
    /// authority may still hold rows worth recovering by hand.
    @discardableResult
    static func quarantineStore(at storeURL: URL) -> URL? {
        let fileManager = FileManager.default
        let directory = storeURL.deletingLastPathComponent()
        let storeName = storeURL.lastPathComponent
        let siblings = (try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        // SwiftData keeps its SQLite sidecars as "<store>-shm" and "<store>-wal".
        let storeFiles = siblings.filter { $0.lastPathComponent.hasPrefix(storeName) }
        guard !storeFiles.isEmpty else { return nil }

        let quarantineRoot = directory.appendingPathComponent(quarantineDirectoryName, isDirectory: true)
        let destination = quarantineRoot.appendingPathComponent(quarantineStamp(), isDirectory: true)
        do {
            try fileManager.createDirectory(at: destination, withIntermediateDirectories: true)
        } catch {
            logger.error("Could not create projection cache quarantine: \(describe(error), privacy: .public)")
            return nil
        }

        var cleared = false
        for file in storeFiles {
            do {
                try fileManager.moveItem(at: file, to: destination.appendingPathComponent(file.lastPathComponent))
                cleared = true
            } catch {
                // A file we cannot move is one we also cannot open. Removing it
                // beats leaving the app unable to launch.
                do {
                    try fileManager.removeItem(at: file)
                    cleared = true
                } catch {
                    logger.error("Could not clear projection cache file: \(describe(error), privacy: .public)")
                }
            }
        }

        guard cleared else {
            try? fileManager.removeItem(at: destination)
            return nil
        }

        pruneQuarantine(at: quarantineRoot)
        return destination
    }

    /// Keeps only the newest `maxQuarantinedStores` quarantined caches. Folder
    /// names are ISO 8601 stamps, so lexicographic order is chronological.
    static func pruneQuarantine(at quarantineRoot: URL) {
        let fileManager = FileManager.default
        let entries = (try? fileManager.contentsOfDirectory(at: quarantineRoot, includingPropertiesForKeys: nil)) ?? []
        let ordered = entries
            .map(\.lastPathComponent)
            .sorted(by: >)
            .dropFirst(maxQuarantinedStores)
        for stale in ordered {
            try? fileManager.removeItem(at: quarantineRoot.appendingPathComponent(stale, isDirectory: true))
        }
    }

    private static func quarantineStamp() -> String {
        let formatter = ISO8601DateFormatter()
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.string(from: Date()).replacingOccurrences(of: ":", with: "-")
    }

    private static func describe(_ error: Error) -> String {
        let nsError = error as NSError
        return "\(nsError.domain) \(nsError.code): \(nsError.localizedDescription)"
    }
}

private struct ProjectionStoreRecoveryKey: EnvironmentKey {
    static let defaultValue = ProjectionStoreRecovery.opened
}

extension EnvironmentValues {
    var projectionStoreRecovery: ProjectionStoreRecovery {
        get { self[ProjectionStoreRecoveryKey.self] }
        set { self[ProjectionStoreRecoveryKey.self] = newValue }
    }
}

/// Shown instead of the app shell when no projection cache could be created.
/// The logbook itself is held by Rust, so this is a cache problem to report,
/// not lost data.
struct ProjectionStoreUnavailableView: View {
    let recovery: ProjectionStoreRecovery

    var body: some View {
        ContentUnavailableView {
            Label("Local Cache Unavailable", systemImage: "externaldrive.badge.exclamationmark")
        } description: {
            VStack(spacing: 8) {
                Text("KE8YGW Logger could not open or rebuild its local cache, so the logbook cannot be displayed. Your log is stored separately and has not been deleted.")
                if let reason = recovery.reason {
                    Text(reason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}
