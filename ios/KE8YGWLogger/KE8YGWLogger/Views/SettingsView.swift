import SwiftData
import SwiftUI

struct SettingsView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var settings: [AppSettings]

    var body: some View {
        Form {
            if let appSettings = settings.first {
                Section("Logging Defaults") {
                    TextField("Default Band", text: bind(appSettings, \.defaultBand))
                        .textInputAutocapitalization(.never)
                    TextField("Default Mode", text: bind(appSettings, \.defaultMode))
                        .textInputAutocapitalization(.characters)
                    Toggle("Auto-uppercase callsigns", isOn: bind(appSettings, \.autoUppercaseCallsigns))
                    Toggle("Ask for location/grid later", isOn: bind(appSettings, \.askForLocationLater))
                }

                Section("Future Integrations") {
                    Text("TODO: QRZ lookup.")
                    Text("TODO: LoTW, eQSL, Club Log, and QRZ Logbook integrations.")
                    Text("TODO: Field Day mode.")
                    Text("TODO: Net control mode.")
                    Text("TODO: Local network event sync.")
                    Text("TODO: iCloud sync.")
                    Text("TODO: Map and propagation features.")
                    Text("TODO: TestFlight, Fastlane, and GitHub Actions setup.")
                }
                .foregroundStyle(.secondary)
            } else {
                Button("Create Default Settings") {
                    modelContext.insert(AppSettings())
                    try? modelContext.save()
                }
            }
        }
        .navigationTitle("Settings")
    }

    private func bind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, String>) -> Binding<String> {
        Binding {
            settings[keyPath: keyPath]
        } set: { value in
            settings[keyPath: keyPath] = value
            settings.updatedAt = Date()
            try? modelContext.save()
        }
    }

    private func bind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, Bool>) -> Binding<Bool> {
        Binding {
            settings[keyPath: keyPath]
        } set: { value in
            settings[keyPath: keyPath] = value
            settings.updatedAt = Date()
            try? modelContext.save()
        }
    }
}
