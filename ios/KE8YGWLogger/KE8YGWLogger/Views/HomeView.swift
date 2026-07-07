import SwiftUI

struct HomeView: View {
    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text("KE8YGW Logger")
                        .font(.largeTitle.bold())
                    Text("Offline-first amateur radio logging for iPhone and iPad.")
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 8)
            }

            Section("Quick Actions") {
                NavigationLink("New QSO", destination: NewQSOView())
                NavigationLink("Logbook", destination: LogbookView())
                NavigationLink("Station Profile", destination: StationProfileView())
                NavigationLink("Export Logs", destination: ExportView())
                NavigationLink("Settings", destination: SettingsView())
            }

            Section("Future Modes") {
                Label("POTA/SOTA activation workflow", systemImage: "antenna.radiowaves.left.and.right")
                Label("Net control mode", systemImage: "person.3.sequence")
                Label("Local network event sync", systemImage: "network")
            }
            .foregroundStyle(.secondary)
        }
        .navigationTitle("Home")
    }
}
