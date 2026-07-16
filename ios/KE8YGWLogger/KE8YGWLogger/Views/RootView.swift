import SwiftData
import SwiftUI

struct RootView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @Query private var settings: [AppSettings]
    @StateObject private var bridge = RustBridgeStore()
    @StateObject private var location = LocationCoordinator()

    var body: some View {
        AppShellView()
            .environmentObject(bridge)
        .task {
            await bootstrap()
        }
        .onChange(of: location.currentGrid) { _, newValue in
            guard let appSettings = settings.first, let newValue else { return }
            appSettings.lastGPSGrid = newValue
            appSettings.lastGPSGridAt = Date()
            if !appSettings.effectiveManualGridOverride {
                appSettings.maidenheadGrid = newValue
                appSettings.lastLocationSource = MaidenheadLocationSource.gps.rawValue
            }
            appSettings.updatedAt = Date()
            try? modelContext.save()
            Task {
                do {
                    let result = try await bridge.saveSettings(appSettings.rustSettingsPayload())
                    if let persisted = result.settings {
                        appSettings.apply(rust: persisted)
                        try? modelContext.save()
                    }
                } catch {
                    bridge.lastError = error.localizedDescription
                }
            }
        }
    }

    private func bootstrap() async {
        await cacheExistingRustSettingsIfAvailable()
        await bridge.refreshAll()
        do {
            try ProjectionRefreshService.rebuildStationBook(
                from: bridge.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
        } catch {
            bridge.lastError = error.localizedDescription
        }
        if settings.first?.effectiveUseDeviceLocation == true {
            location.requestCurrentGrid(useDeviceLocation: true)
        }
    }

    private func cacheExistingRustSettingsIfAvailable() async {
        do {
            let result = try await bridge.loadSettings()
            guard result.exists, let rustSettings = result.settings else { return }
            let cache = settings.first ?? AppSettings()
            if settings.first == nil {
                modelContext.insert(cache)
            }
            cache.apply(rust: rustSettings)
            for duplicate in settings.dropFirst() {
                modelContext.delete(duplicate)
            }
            try? modelContext.save()
        } catch {
            bridge.lastError = error.localizedDescription
        }
    }
}
