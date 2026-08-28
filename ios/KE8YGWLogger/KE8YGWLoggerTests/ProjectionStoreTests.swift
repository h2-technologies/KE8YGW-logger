import Foundation
import SwiftData
import XCTest
@testable import KE8YGWLogger

private struct ProjectionStoreTestError: Error {}

final class ProjectionStoreTests: XCTestCase {
    private var workingDirectory: URL!

    override func setUpWithError() throws {
        try super.setUpWithError()
        workingDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("ProjectionStoreTests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: workingDirectory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: workingDirectory)
        workingDirectory = nil
        try super.tearDownWithError()
    }

    // MARK: - Recovery ladder

    func testOpensCleanlyWhenTheStoreIsHealthy() throws {
        let schema = Schema(ProjectionStoreFactory.projectionModels)
        let store = ProjectionStoreFactory.make(
            schema: schema,
            configuration: ModelConfiguration(schema: schema, url: storeURL())
        ) { schema, configuration in
            try ModelContainer(for: schema, configurations: configuration)
        }

        XCTAssertNotNil(store.container)
        XCTAssertEqual(store.recovery, .opened)
        XCTAssertFalse(store.recovery.requiresFullProjectionRebuild)
    }

    func testQuarantinesAndRebuildsWhenTheStoreCannotBeOpened() throws {
        let schema = Schema(ProjectionStoreFactory.projectionModels)
        let url = storeURL()
        try writeStoreFiles(at: url)

        var attempts = 0
        let store = ProjectionStoreFactory.make(
            schema: schema,
            configuration: ModelConfiguration(schema: schema, url: url)
        ) { schema, configuration in
            attempts += 1
            // Fail only the first open, the way an incompatible on-disk store does.
            if attempts == 1 { throw ProjectionStoreTestError() }
            return try ModelContainer(for: schema, configurations: configuration)
        }

        XCTAssertNotNil(store.container)
        XCTAssertTrue(store.recovery.requiresFullProjectionRebuild)
        guard case .rebuiltAfterFailure(_, let quarantine) = store.recovery else {
            return XCTFail("expected a rebuilt store, got \(store.recovery)")
        }
        let quarantineURL = try XCTUnwrap(quarantine)
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: quarantineURL.appendingPathComponent(url.lastPathComponent).path
        ))
    }

    func testFallsBackToMemoryWhenTheDiskStoreKeepsFailing() throws {
        let schema = Schema(ProjectionStoreFactory.projectionModels)
        let store = ProjectionStoreFactory.make(
            schema: schema,
            configuration: ModelConfiguration(schema: schema, url: storeURL())
        ) { schema, configuration in
            guard configuration.isStoredInMemoryOnly else { throw ProjectionStoreTestError() }
            return try ModelContainer(for: schema, configurations: configuration)
        }

        XCTAssertNotNil(store.container)
        XCTAssertTrue(store.recovery.requiresFullProjectionRebuild)
        guard case .inMemoryFallback = store.recovery else {
            return XCTFail("expected an in-memory fallback, got \(store.recovery)")
        }
    }

    func testReportsUnavailableRatherThanTrappingWhenNoContainerCanBeMade() throws {
        let schema = Schema(ProjectionStoreFactory.projectionModels)
        let store = ProjectionStoreFactory.make(
            schema: schema,
            configuration: ModelConfiguration(schema: schema, url: storeURL())
        ) { _, _ in
            throw ProjectionStoreTestError()
        }

        XCTAssertNil(store.container)
        XCTAssertTrue(store.recovery.requiresFullProjectionRebuild)
        guard case .unavailable = store.recovery else {
            return XCTFail("expected an unavailable store, got \(store.recovery)")
        }
    }

    // MARK: - Quarantine

    func testQuarantineMovesTheStoreAndItsSidecars() throws {
        let url = storeURL()
        try writeStoreFiles(at: url)

        let quarantine = try XCTUnwrap(ProjectionStoreFactory.quarantineStore(at: url))

        for suffix in ["", "-shm", "-wal"] {
            let name = url.lastPathComponent + suffix
            XCTAssertFalse(
                FileManager.default.fileExists(atPath: workingDirectory.appendingPathComponent(name).path),
                "\(name) should have been moved out of the active store directory"
            )
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: quarantine.appendingPathComponent(name).path),
                "\(name) should have been preserved in quarantine"
            )
        }
    }

    func testQuarantineIsANoOpWhenThereIsNoStoreOnDisk() {
        XCTAssertNil(ProjectionStoreFactory.quarantineStore(at: storeURL()))
    }

    func testPruneKeepsOnlyTheNewestQuarantinedStores() throws {
        let root = workingDirectory.appendingPathComponent(
            ProjectionStoreFactory.quarantineDirectoryName,
            isDirectory: true
        )
        // ISO 8601 stamps sort lexicographically, which is what pruning relies on.
        let stamps = (1...ProjectionStoreFactory.maxQuarantinedStores + 2).map {
            String(format: "2026-08-%02dT00-00-00Z", $0)
        }
        for stamp in stamps {
            try FileManager.default.createDirectory(
                at: root.appendingPathComponent(stamp, isDirectory: true),
                withIntermediateDirectories: true
            )
        }

        ProjectionStoreFactory.pruneQuarantine(at: root)

        let remaining = try FileManager.default
            .contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
            .map(\.lastPathComponent)
            .sorted()
        XCTAssertEqual(remaining, Array(stamps.suffix(ProjectionStoreFactory.maxQuarantinedStores)))
    }

    // MARK: - Helpers

    private func storeURL() -> URL {
        workingDirectory.appendingPathComponent("projection.store")
    }

    private func writeStoreFiles(at url: URL) throws {
        for suffix in ["", "-shm", "-wal"] {
            let sidecar = URL(fileURLWithPath: url.path + suffix)
            try Data("not a usable sqlite store".utf8).write(to: sidecar)
        }
    }
}
