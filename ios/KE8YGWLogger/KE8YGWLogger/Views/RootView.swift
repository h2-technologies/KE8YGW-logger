import SwiftData
import SwiftUI

struct RootView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var profiles: [StationProfile]
    @Query private var settings: [AppSettings]

    var body: some View {
        NavigationStack {
            HomeView()
        }
        .task {
            seedDefaultsIfNeeded()
        }
    }

    private func seedDefaultsIfNeeded() {
        if profiles.isEmpty {
            modelContext.insert(StationProfile())
        }
        if settings.isEmpty {
            modelContext.insert(AppSettings())
        }
        try? modelContext.save()
    }
}
