import SwiftData
import SwiftUI

struct RootView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @Query private var settings: [AppSettings]
    @StateObject private var bridge = RustBridgeStore()

    var body: some View {
        AppShellView()
            .environmentObject(bridge)
        .task {
            await bootstrap()
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
    }

    private func seedLocalSettingsIfNeeded() {
        if settings.isEmpty {
            modelContext.insert(AppSettings())
            try? modelContext.save()
        }
    }
}
