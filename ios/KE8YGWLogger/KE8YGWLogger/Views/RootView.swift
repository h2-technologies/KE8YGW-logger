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
        }
    }

    private func bootstrap() async {
        seedLocalSettingsIfNeeded()
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

    private func seedLocalSettingsIfNeeded() {
        if settings.isEmpty {
            modelContext.insert(AppSettings())
            try? modelContext.save()
        } else {
            var migrated = false
            for item in settings {
                let before = item.updatedAt
                item.migrateIfNeeded()
                migrated = migrated || item.updatedAt != before
            }
            if migrated {
                try? modelContext.save()
            }
        }
    }
}
