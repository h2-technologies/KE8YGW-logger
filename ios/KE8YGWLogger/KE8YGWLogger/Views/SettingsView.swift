import SwiftData
import SwiftUI

struct SettingsView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var settings: [AppSettings]
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @StateObject private var notifications = NotificationCoordinator()
    @StateObject private var location = LocationCoordinator()
    @State private var locationMessage: String?

    private var appSettings: AppSettings? { settings.first }
    private var visibleProfiles: [StationProfile] { profiles.filter { !$0.isTombstoned } }
    private var visibleEquipment: [StationEquipment] { equipment.filter { !$0.isTombstoned } }

    var body: some View {
        Form {
            if let appSettings {
                Section("General") {
                    Picker("Theme", selection: bind(appSettings, \.appearance)) {
                        Text("System").tag("system")
                        Text("Light").tag("light")
                        Text("Dark").tag("dark")
                    }
                    TextField("Accent Color", text: bind(appSettings, \.accentColorName))
                        .textInputAutocapitalization(.never)
                    TextField("Operator Callsign", text: uppercaseBind(appSettings, \.operatorCallsign))
                        .textInputAutocapitalization(.characters)
                    TextField("Operator Name", text: optionalBind(appSettings, \.operatorName))
                    TextField("Operator Email", text: optionalBind(appSettings, \.operatorEmail))
                        .textInputAutocapitalization(.never)
                        .keyboardType(.emailAddress)
                    TextField("Station Callsign", text: uppercaseBind(appSettings, \.stationCallsign))
                        .textInputAutocapitalization(.characters)
                }

                Section("Station Defaults") {
                    Picker("Default Station Profile", selection: optionalBind(appSettings, \.defaultStationProfileID)) {
                        Text("None").tag("")
                        ForEach(visibleProfiles) { profile in
                            Text(profile.displayName).tag(profile.canonicalID.isEmpty ? profile.id.uuidString : profile.canonicalID)
                        }
                    }
                    Picker("Default Equipment", selection: optionalBind(appSettings, \.defaultEquipmentProfileID)) {
                        Text("None").tag("")
                        ForEach(visibleEquipment) { item in
                            Text(item.displayName.isEmpty ? item.equipmentType.capitalized : item.displayName)
                                .tag(item.canonicalID.isEmpty ? item.id.uuidString : item.canonicalID)
                        }
                    }
                    NavigationLink("Manage Station Profiles", destination: StationManagementView())
                }

                Section("Location") {
                    Toggle("Use Device Location", isOn: Binding {
                        appSettings.effectiveUseDeviceLocation
                    } set: { value in
                        appSettings.effectiveUseDeviceLocation = value
                        appSettings.updatedAt = Date()
                        try? modelContext.save()
                        if value {
                            location.requestCurrentGrid(useDeviceLocation: true)
                        } else {
                            location.stopUsingLocation()
                        }
                    })
                    Toggle("Use Manual Grid Override", isOn: Binding {
                        appSettings.effectiveManualGridOverride
                    } set: { value in
                        appSettings.effectiveManualGridOverride = value
                        appSettings.lastLocationSource = value ? MaidenheadLocationSource.manual.rawValue : MaidenheadLocationSource.stationDefault.rawValue
                        save(appSettings)
                    })
                    TextField("Manual Maidenhead Grid", text: gridBind(appSettings))
                        .textInputAutocapitalization(.characters)
                    TextField("Location Name / QTH", text: optionalBind(appSettings, \.manualLocationName))
                    TextField("County", text: optionalBind(appSettings, \.manualCounty))
                    TextField("State", text: uppercaseOptionalBind(appSettings, \.manualState))
                    TextField("Country", text: optionalBind(appSettings, \.manualCountry))
                    Button("Update GPS Grid") {
                        location.requestCurrentGrid(useDeviceLocation: appSettings.effectiveUseDeviceLocation)
                    }
                    .disabled(!appSettings.effectiveUseDeviceLocation)
                    DetailRow(title: "Location Permission", value: location.permissionState.label)
                    DetailRow(title: "Current Source", value: currentLocationSource(appSettings).label)
                    DetailRow(title: "Effective Grid", value: effectiveGrid(appSettings))
                    Text(location.statusMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text("iOS location permission can also be changed in the system Settings app.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if let locationMessage {
                        Text(locationMessage)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }

                Section("Provider Credentials") {
                    ForEach(ProviderCredentialCatalog.definitions) { definition in
                        NavigationLink {
                            ProviderCredentialFormView(settings: appSettings, definition: definition)
                        } label: {
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(definition.displayName)
                                    Text(providerCredentialSummary(appSettings, definition.id))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                ProviderStatusBadge(settings: appSettings, providerID: definition.id)
                            }
                        }
                    }
                }

                Section("Provider Behavior") {
                    NavigationLink("Provider Status and Enablement", destination: ProviderStatusView())
                    Toggle("Provider Notifications", isOn: bind(appSettings, \.providerNotificationsEnabled))
                    DetailRow(title: "Notification Access", value: String(describing: notifications.authorizationStatus))
                    Button("Enable Local Notifications") {
                        Task { await notifications.requestAuthorization() }
                    }
                }

                Section("Sync and Server") {
                    NavigationLink("Sync Status", destination: SyncWorkspaceView())
                    TextField("Server URL", text: optionalBind(appSettings, \.serverURL))
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                    Toggle("Background Sync", isOn: bind(appSettings, \.backgroundSyncEnabled))
                    Toggle("Prefer LAN Sync", isOn: optionalBoolBind(appSettings, \.preferLANSync, defaultValue: true))
                    Toggle("Auto Push", isOn: optionalBoolBind(appSettings, \.autoPushSync, defaultValue: false))
                    Toggle("Auto Pull", isOn: optionalBoolBind(appSettings, \.autoPullSync, defaultValue: false))
                    Stepper("Sync Interval: \(appSettings.syncIntervalMinutes ?? 15) min", value: optionalIntBind(appSettings, \.syncIntervalMinutes, defaultValue: 15), in: 1...240)
                }

                Section("Logging and Workflow") {
                    TextField("Default Band", text: bind(appSettings, \.defaultBand))
                        .textInputAutocapitalization(.never)
                    TextField("Default Mode", text: uppercaseBind(appSettings, \.defaultMode))
                        .textInputAutocapitalization(.characters)
                    Toggle("Auto-uppercase callsigns", isOn: bind(appSettings, \.autoUppercaseCallsigns))
                    Toggle("Ask for location/grid later", isOn: bind(appSettings, \.askForLocationLater))
                    Toggle("Allow Offline Activations", isOn: optionalBoolBind(appSettings, \.allowOfflineActivations, defaultValue: true))
                    Stepper("Credential Validation TTL: \(appSettings.validationTTLHours ?? 24) h", value: optionalIntBind(appSettings, \.validationTTLHours, defaultValue: 24), in: 1...720)
                    TextField("Activation Notes Template", text: optionalBind(appSettings, \.activationNotesTemplate), axis: .vertical)
                        .lineLimit(2...5)
                }

                Section("Net Control") {
                    TextField("Default Net Name", text: optionalBind(appSettings, \.netDefaultName))
                    TextField("Default Frequency MHz", text: optionalBind(appSettings, \.netDefaultFrequencyMHz))
                        .keyboardType(.decimalPad)
                    TextField("Default Mode", text: uppercaseOptionalBind(appSettings, \.netDefaultMode))
                        .textInputAutocapitalization(.characters)
                    NavigationLink("Open Net Control", destination: NetControlView())
                }

                Section("Privacy and Diagnostics") {
                    Toggle("Include Logs in Diagnostics", isOn: bind(appSettings, \.shareDiagnosticsWithLogs))
                    NavigationLink("Diagnostics", destination: DiagnosticsView())
                    NavigationLink("Backup & Restore", destination: BackupRestoreView())
                }

                Section("Advanced") {
                    Toggle("Developer Settings", isOn: bind(appSettings, \.developerModeEnabled))
                    DetailRow(title: "Rust Bridge", value: bridge.client.isLive ? "Live" : "Fallback")
                    DetailRow(title: "Settings Schema", value: "\(appSettings.settingsSchemaVersion ?? 1)")
                }

                Section("About") {
                    DetailRow(title: "App", value: "KE8YGW Logger")
                    DetailRow(title: "Version", value: "0.2.0")
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
            if let appSettings {
                appSettings.migrateIfNeeded()
                try? modelContext.save()
                if appSettings.effectiveUseDeviceLocation {
                    location.requestCurrentGrid(useDeviceLocation: true)
                }
            }
            await notifications.refreshAuthorizationStatus()
        }
        .onChange(of: location.currentGrid) { _, newValue in
            guard let appSettings, let newValue else { return }
            appSettings.lastGPSGrid = newValue
            appSettings.lastGPSGridAt = Date()
            if !appSettings.effectiveManualGridOverride {
                appSettings.maidenheadGrid = newValue
                appSettings.lastLocationSource = MaidenheadLocationSource.gps.rawValue
            } else {
                appSettings.lastLocationSource = MaidenheadLocationSource.manual.rawValue
            }
            save(appSettings)
        }
    }

    private func bind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, String>) -> Binding<String> {
        Binding {
            settings[keyPath: keyPath]
        } set: { value in
            settings[keyPath: keyPath] = value
            save(settings)
        }
    }

    private func uppercaseBind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, String>) -> Binding<String> {
        Binding {
            settings[keyPath: keyPath]
        } set: { value in
            settings[keyPath: keyPath] = value.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            save(settings)
        }
    }

    private func optionalBind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, String?>) -> Binding<String> {
        Binding {
            settings[keyPath: keyPath] ?? ""
        } set: { value in
            settings[keyPath: keyPath] = value
            save(settings)
        }
    }

    private func uppercaseOptionalBind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, String?>) -> Binding<String> {
        Binding {
            settings[keyPath: keyPath] ?? ""
        } set: { value in
            settings[keyPath: keyPath] = value.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            save(settings)
        }
    }

    private func gridBind(_ settings: AppSettings) -> Binding<String> {
        Binding {
            settings.maidenheadGrid ?? ""
        } set: { value in
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty {
                settings.maidenheadGrid = ""
                locationMessage = nil
            } else if let normalized = HamRadioUtilities.normalizedMaidenhead(trimmed) {
                settings.maidenheadGrid = normalized
                settings.lastLocationSource = MaidenheadLocationSource.manual.rawValue
                locationMessage = nil
            } else {
                settings.maidenheadGrid = trimmed.uppercased()
                locationMessage = "Enter a valid 4- or 6-character Maidenhead grid."
            }
            save(settings)
        }
    }

    private func bind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, Bool>) -> Binding<Bool> {
        Binding {
            settings[keyPath: keyPath]
        } set: { value in
            settings[keyPath: keyPath] = value
            save(settings)
        }
    }

    private func optionalBoolBind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, Bool?>, defaultValue: Bool) -> Binding<Bool> {
        Binding {
            settings[keyPath: keyPath] ?? defaultValue
        } set: { value in
            settings[keyPath: keyPath] = value
            save(settings)
        }
    }

    private func optionalIntBind(_ settings: AppSettings, _ keyPath: ReferenceWritableKeyPath<AppSettings, Int?>, defaultValue: Int) -> Binding<Int> {
        Binding {
            settings[keyPath: keyPath] ?? defaultValue
        } set: { value in
            settings[keyPath: keyPath] = value
            save(settings)
        }
    }

    private func save(_ settings: AppSettings) {
        settings.updatedAt = Date()
        try? modelContext.save()
    }

    private func currentLocationSource(_ settings: AppSettings) -> MaidenheadLocationSource {
        MaidenheadLocationSource(rawValue: settings.lastLocationSource ?? "") ?? .unknown
    }

    private func effectiveGrid(_ settings: AppSettings) -> String {
        if settings.effectiveManualGridOverride, let manual = HamRadioUtilities.normalizedMaidenhead(settings.maidenheadGrid ?? "") {
            return "\(manual) (Manual)"
        }
        if settings.effectiveUseDeviceLocation, let gps = HamRadioUtilities.normalizedMaidenhead(settings.lastGPSGrid ?? "") {
            return "\(gps) (GPS)"
        }
        if let manual = HamRadioUtilities.normalizedMaidenhead(settings.maidenheadGrid ?? "") {
            return "\(manual) (Station/Manual)"
        }
        return "Unset"
    }

    private func providerCredentialSummary(_ settings: AppSettings, _ providerID: String) -> String {
        let validation = settings.providerValidationRecord(providerID)
        if validation.validated, let date = validation.validatedAt {
            return "Validated \(date.formatted(date: .abbreviated, time: .shortened))"
        }
        return validation.configured ? "Configured, validation required" : "Not configured"
    }
}

private struct ProviderStatusBadge: View {
    var settings: AppSettings
    var providerID: String

    var body: some View {
        let validation = settings.providerValidationRecord(providerID)
        Label(validation.validated ? "Validated" : validation.configured ? "Configured" : "Missing",
              systemImage: validation.validated ? "checkmark.seal" : validation.configured ? "key" : "exclamationmark.circle")
            .labelStyle(.iconOnly)
            .foregroundStyle(validation.validated ? .green : validation.configured ? .orange : .secondary)
            .accessibilityLabel(validation.validated ? "Credential validated" : validation.configured ? "Credential configured but not validated" : "Credential missing")
    }
}

struct ProviderCredentialFormView: View {
    @Environment(\.modelContext) private var modelContext
    @Bindable var settings: AppSettings
    var definition: ProviderCredentialDefinition

    @State private var fieldValues: [String: String] = [:]
    @State private var message: String?
    @State private var confirmRemoval = false
    private let vault = KeychainCredentialVault()

    var body: some View {
        Form {
            Section("Status") {
                Toggle("Provider Enabled", isOn: Binding {
                    settings.isProviderEnabled(definition.id)
                } set: { value in
                    settings.setProviderEnabled(definition.id, enabled: value)
                    try? modelContext.save()
                })
                let validation = settings.providerValidationRecord(definition.id)
                DetailRow(title: "Configured", value: validation.configured ? "Yes" : "No")
                DetailRow(title: "Validated", value: validation.validated ? "Yes" : "No")
                DetailRow(title: "Last Validation", value: validation.validatedAt?.formatted(date: .abbreviated, time: .shortened) ?? "Never")
                DetailRow(title: "Last Result", value: validation.message)
            }

            Section("Credential Fields") {
                ForEach(definition.fields) { field in
                    if field.kind == .secure {
                        SecureField(field.label, text: binding(for: field.id))
                            .textInputAutocapitalization(.never)
                    } else {
                        TextField(field.label, text: binding(for: field.id))
                            .textInputAutocapitalization(field.id.contains("callsign") ? .characters : .never)
                            .keyboardType(field.kind == .number ? .numberPad : field.kind == .url ? .URL : .default)
                    }
                }
                Text("Secrets are saved in iOS Keychain. Settings stores only provider status and non-secret field metadata.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Actions") {
                Button("Save Credentials", action: saveCredentials)
                    .disabled(!hasRequiredFieldsForSave)
                Button("Validate Credentials", action: validateCredentials)
                    .disabled(!settings.providerValidationRecord(definition.id).configured)
                Button("Remove Credentials", role: .destructive) {
                    confirmRemoval = true
                }
                .disabled(!settings.providerValidationRecord(definition.id).configured)
                if let message {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(definition.displayName)
        .onAppear(perform: loadMetadata)
        .confirmationDialog("Remove \(definition.displayName) credentials?", isPresented: $confirmRemoval, titleVisibility: .visible) {
            Button("Remove Credentials", role: .destructive, action: removeCredentials)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Stored Keychain secrets for this provider will be deleted. Provider history and status rows are preserved.")
        }
    }

    private var hasRequiredFieldsForSave: Bool {
        definition.fields.allSatisfy { field in
            if field.kind == .secure {
                return !(fieldValues[field.id] ?? "").isEmpty || settings.providerCredentialMetadata(definition.id)["\(field.id)_configured"] == "true"
            }
            return !field.required || !(fieldValues[field.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    private func binding(for fieldID: String) -> Binding<String> {
        Binding {
            fieldValues[fieldID] ?? ""
        } set: { value in
            fieldValues[fieldID] = value
        }
    }

    private func loadMetadata() {
        fieldValues = settings.providerCredentialMetadata(definition.id).filter { !$0.key.hasSuffix("_configured") }
    }

    private func saveCredentials() {
        do {
            var metadata = settings.providerCredentialMetadata(definition.id)
            for field in definition.fields {
                let value = (fieldValues[field.id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                if field.kind == .secure {
                    if !value.isEmpty {
                        try vault.save(secret: value, account: field.id, providerId: definition.id)
                        metadata["\(field.id)_configured"] = "true"
                        fieldValues[field.id] = ""
                    }
                } else {
                    metadata[field.id] = field.id.contains("callsign") ? value.uppercased() : value
                }
            }
            settings.setProviderCredentialMetadata(definition.id, metadata: metadata)
            settings.setProviderValidationRecord(definition.id, record: ProviderValidationRecord(
                configured: true,
                validated: false,
                validatedAt: nil,
                message: "Saved. Validate before online provider use."
            ))
            try modelContext.save()
            message = "Saved \(definition.displayName) credentials."
        } catch {
            message = error.localizedDescription
        }
    }

    private func validateCredentials() {
        let metadata = settings.providerCredentialMetadata(definition.id)
        let missing = definition.fields.filter { field in
            if field.kind == .secure {
                return metadata["\(field.id)_configured"] != "true"
            }
            return field.required && (metadata[field.id] ?? "").isEmpty
        }
        if missing.isEmpty {
            settings.setProviderValidationRecord(definition.id, record: ProviderValidationRecord(
                configured: true,
                validated: true,
                validatedAt: Date(),
                message: "Credential configuration validated locally. Live provider auth will be rechecked when the Rust adapter is available."
            ))
            message = "Validated saved \(definition.displayName) credential configuration."
        } else {
            settings.setProviderValidationRecord(definition.id, record: ProviderValidationRecord(
                configured: true,
                validated: false,
                validatedAt: nil,
                message: "Missing required fields: \(missing.map(\.label).joined(separator: ", "))"
            ))
            message = "Missing required fields."
        }
        try? modelContext.save()
    }

    private func removeCredentials() {
        for field in definition.secureFields {
            try? vault.delete(account: field.id, providerId: definition.id)
        }
        settings.clearProviderCredentialMetadata(definition.id)
        try? modelContext.save()
        fieldValues = [:]
        message = "Removed \(definition.displayName) credentials."
    }
}
