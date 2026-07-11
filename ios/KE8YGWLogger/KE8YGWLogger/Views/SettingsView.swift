import SwiftData
import SwiftUI

struct SettingsView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var settings: [AppSettings]
    @StateObject private var notifications = NotificationCoordinator()

    var body: some View {
        Form {
            if let appSettings = settings.first {
                Section("Appearance") {
                    Picker("Theme", selection: bind(appSettings, \.appearance)) {
                        Text("System").tag("system")
                        Text("Light").tag("light")
                        Text("Dark").tag("dark")
                    }
                    TextField("Accent Color", text: bind(appSettings, \.accentColorName))
                        .textInputAutocapitalization(.never)
                }

                Section("Operator") {
                    TextField("Callsign", text: bind(appSettings, \.operatorCallsign))
                        .textInputAutocapitalization(.characters)
                    TextField("Station", text: bind(appSettings, \.stationCallsign))
                        .textInputAutocapitalization(.characters)
                }

                Section("Logging Defaults") {
                    TextField("Default Band", text: bind(appSettings, \.defaultBand))
                        .textInputAutocapitalization(.never)
                    TextField("Default Mode", text: bind(appSettings, \.defaultMode))
                        .textInputAutocapitalization(.characters)
                    Toggle("Auto-uppercase callsigns", isOn: bind(appSettings, \.autoUppercaseCallsigns))
                    Toggle("Ask for location/grid later", isOn: bind(appSettings, \.askForLocationLater))
                }

                Section("Providers") {
                    NavigationLink("Provider Status", destination: ProviderStatusView())
                    NavigationLink("Credential Manager", destination: ProviderStatusView())
                    Toggle("Provider Notifications", isOn: bind(appSettings, \.providerNotificationsEnabled))
                    DetailRow(title: "Notification Access", value: String(describing: notifications.authorizationStatus))
                    Button("Enable Local Notifications") {
                        Task { await notifications.requestAuthorization() }
                    }
                }

                Section("Sync") {
                    NavigationLink("Sync Status", destination: SyncWorkspaceView())
                    Toggle("Background Sync", isOn: bind(appSettings, \.backgroundSyncEnabled))
                }

                Section("Privacy") {
                    Toggle("Include Logs in Diagnostics", isOn: bind(appSettings, \.shareDiagnosticsWithLogs))
                }

                Section("Diagnostics") {
                    NavigationLink("Diagnostics", destination: DiagnosticsView())
                    NavigationLink("Backup & Restore", destination: BackupRestoreView())
                }

                Section("Developer") {
                    Toggle("Developer Settings", isOn: bind(appSettings, \.developerModeEnabled))
                    DetailRow(title: "Rust Bridge", value: "Loaded through Shared/RustBridge")
                }

                Section("About") {
                    DetailRow(title: "App", value: "KE8YGW Logger")
                    DetailRow(title: "Version", value: "0.1.0")
                }
            } else {
                Button("Create Default Settings") {
                    modelContext.insert(AppSettings())
                    try? modelContext.save()
                }
            }
        }
        .navigationTitle("Settings")
        .task {
            await notifications.refreshAuthorizationStatus()
        }
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
