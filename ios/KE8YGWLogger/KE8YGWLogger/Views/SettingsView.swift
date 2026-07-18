import SwiftData
import SwiftUI

struct SettingsView: View {
    private enum LoadState: Equatable {
        case loading
        case missing
        case loaded
        case failed(String)
    }

    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var settings: [AppSettings]
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @StateObject private var notifications = NotificationCoordinator()
    @StateObject private var location = LocationCoordinator()
    @State private var locationMessage: String?
    @State private var loadState: LoadState = .loading
    @State private var createInProgress = false
    @State private var saveInProgress = false
    @State private var pendingSave = false
    @State private var saveStatus: String?
    @State private var saveError: String?
    @State private var fieldErrors: [String: String] = [:]
    @State private var didInitialLoad = false

    private var appSettings: AppSettings? { settings.first }
    private var visibleProfiles: [StationProfile] { profiles.filter { !$0.isTombstoned } }
    private var visibleEquipment: [StationEquipment] { equipment.filter { !$0.isTombstoned } }

    @ViewBuilder
    private var statusSection: some View {
        if saveInProgress || saveStatus != nil || saveError != nil || !fieldErrors.isEmpty {
            Section("Status") {
                if saveInProgress {
                    Label("Saving", systemImage: "arrow.triangle.2.circlepath")
                } else if let saveStatus {
                    Label(saveStatus, systemImage: "checkmark.circle")
                        .foregroundStyle(.green)
                }
                if let saveError {
                    Text(saveError)
                        .foregroundStyle(.red)
                }
                ForEach(fieldErrors.keys.sorted(), id: \.self) { key in
                    if let error = fieldErrors[key] {
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }
            }
        }
    }

    var body: some View {
        Form {
            switch loadState {
            case .loading:
                Section {
                    ProgressView("Loading settings")
                }
            case .failed(let message):
                Section("Settings Unavailable") {
                    Text(message)
                        .foregroundStyle(.red)
                    Button("Retry") {
                        Task { await loadSettingsFromRust() }
                    }
                }
            case .missing:
                Section("Settings") {
                    Text("No application settings record exists yet.")
                        .foregroundStyle(.secondary)
                    Button {
                        Task { await createDefaultSettings() }
                    } label: {
                        if createInProgress {
                            ProgressView()
                        } else {
                            Text("Create Default Settings")
                        }
                    }
                    .disabled(createInProgress)
                    .accessibilityLabel("Create Default Settings")
                    .accessibilityHint("Creates the canonical Rust-backed settings record and opens the editable Settings page.")
                    if let saveError {
                        Text(saveError)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }
            case .loaded:
                if let appSettings {
                    statusSection
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
                    validationText("operatorCallsign")
                    TextField("Additional Callsigns", text: additionalCallsignsBind(appSettings))
                        .textInputAutocapitalization(.characters)
                        .accessibilityHint("Separate multiple callsigns with commas.")
                    TextField("Operator Name", text: optionalBind(appSettings, \.operatorName))
                    TextField("Operator Email", text: optionalBind(appSettings, \.operatorEmail))
                        .textInputAutocapitalization(.never)
                        .keyboardType(.emailAddress)
                    TextField("Station Callsign", text: uppercaseBind(appSettings, \.stationCallsign))
                        .textInputAutocapitalization(.characters)
                    validationText("stationCallsign")
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
                    validationText("maidenheadGrid")
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
                    validationText("serverURL")
                    Text("Changing the sync server changes future sync traffic only. Existing local log data and device identity are preserved.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Device Name", text: optionalBind(appSettings, \.syncDeviceName))
                    TextField("Account Label", text: optionalBind(appSettings, \.syncAccountLabel))
                    NavigationLink("Manage Sync Credentials") {
                        SyncCredentialFormView(settings: appSettings)
                    }
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
                    Toggle("POTA Uploads Enabled", isOn: optionalBoolBind(appSettings, \.potaUploadEnabled, defaultValue: false))
                    Toggle("SOTA Uploads Enabled", isOn: optionalBoolBind(appSettings, \.sotaUploadEnabled, defaultValue: false))
                }

                Section("Net Control") {
                    TextField("Default Net Name", text: optionalBind(appSettings, \.netDefaultName))
                    TextField("Default Frequency MHz", text: optionalBind(appSettings, \.netDefaultFrequencyMHz))
                        .keyboardType(.decimalPad)
                    TextField("Default Mode", text: uppercaseOptionalBind(appSettings, \.netDefaultMode))
                        .textInputAutocapitalization(.characters)
                    Toggle("Sort Roster by Traffic Priority", isOn: optionalBoolBind(appSettings, \.sortNetRosterByTrafficPriority, defaultValue: true))
                    NavigationLink("Open Net Control", destination: NetControlView())
                }

                Section("Maps and Display") {
                    TextField("Default Map Layer", text: optionalBind(appSettings, \.mapDefaultLayer))
                    Toggle("Show QSO Map Objects", isOn: optionalBoolBind(appSettings, \.showQSOMapObjects, defaultValue: true))
                    Toggle("Show Station Markers", isOn: optionalBoolBind(appSettings, \.showStationMapMarkers, defaultValue: true))
                    NavigationLink("Open Maps", destination: MapWorkspaceView())
                }

                Section("Privacy and Diagnostics") {
                    Toggle("Include Diagnostics in Backups", isOn: optionalBoolBind(appSettings, \.includeDiagnosticsInBackups, defaultValue: false))
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
                }
            }
        }
        .navigationTitle("Settings")
        .task {
            if !didInitialLoad {
                didInitialLoad = true
                await loadSettingsFromRust()
            }
            if let appSettings, loadState == .loaded {
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
                fieldErrors.removeValue(forKey: "maidenheadGrid")
            } else if let normalized = HamRadioUtilities.normalizedMaidenhead(trimmed) {
                settings.maidenheadGrid = normalized
                settings.lastLocationSource = MaidenheadLocationSource.manual.rawValue
                locationMessage = nil
                fieldErrors.removeValue(forKey: "maidenheadGrid")
            } else {
                settings.maidenheadGrid = trimmed.uppercased()
                locationMessage = "Enter a valid 4- or 6-character Maidenhead grid."
                fieldErrors["maidenheadGrid"] = "Enter a valid 4- or 6-character Maidenhead grid."
            }
            save(settings)
        }
    }

    private func additionalCallsignsBind(_ settings: AppSettings) -> Binding<String> {
        Binding {
            settings.additionalCallsigns.joined(separator: ", ")
        } set: { value in
            let callsigns = value
                .split(separator: ",")
                .map { HamRadioUtilities.normalizeCallsign(String($0)) }
                .filter { !$0.isEmpty }
            settings.setAdditionalCallsigns(callsigns)
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
        guard validate(settings) else { return }
        settings.updatedAt = Date()
        try? modelContext.save()
        pendingSave = true
        drainSaveQueue(settings)
    }

    @discardableResult
    private func validate(_ settings: AppSettings) -> Bool {
        if HamRadioUtilities.isValidCallsign(settings.operatorCallsign) {
            fieldErrors.removeValue(forKey: "operatorCallsign")
        } else {
            fieldErrors["operatorCallsign"] = "Enter a valid primary callsign."
        }
        if HamRadioUtilities.isValidCallsign(settings.stationCallsign) {
            fieldErrors.removeValue(forKey: "stationCallsign")
        } else {
            fieldErrors["stationCallsign"] = "Enter a valid station callsign."
        }
        let invalidAdditional = settings.additionalCallsigns.first { !HamRadioUtilities.isValidCallsign($0) }
        if let invalidAdditional {
            fieldErrors["additionalCallsigns"] = "Additional callsign \(invalidAdditional) is not valid."
        } else {
            fieldErrors.removeValue(forKey: "additionalCallsigns")
        }
        if let grid = settings.maidenheadGrid, !grid.isEmpty, HamRadioUtilities.normalizedMaidenhead(grid) == nil {
            fieldErrors["maidenheadGrid"] = "Enter a valid 4- or 6-character Maidenhead grid."
        } else if fieldErrors["maidenheadGrid"] != nil {
            fieldErrors.removeValue(forKey: "maidenheadGrid")
        }
        if isValidSyncURL(settings.serverURL ?? "") {
            fieldErrors.removeValue(forKey: "serverURL")
        } else {
            fieldErrors["serverURL"] = "Enter an http or https sync server URL with a host."
        }
        return fieldErrors.isEmpty
    }

    private func isValidSyncURL(_ value: String) -> Bool {
        guard let url = URL(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = url.scheme?.lowercased(),
              ["http", "https"].contains(scheme),
              url.host?.isEmpty == false else {
            return false
        }
        return true
    }

    @ViewBuilder
    private func validationText(_ key: String) -> some View {
        if let error = fieldErrors[key] {
            Text(error)
                .font(.caption)
                .foregroundStyle(.red)
        }
    }

    private func drainSaveQueue(_ settings: AppSettings) {
        guard !saveInProgress else { return }
        saveInProgress = true
        saveStatus = nil
        saveError = nil
        Task {
            while pendingSave {
                pendingSave = false
                do {
                    let result = try await bridge.saveSettings(settings.rustSettingsPayload())
                    if let persisted = result.settings {
                        settings.apply(rust: persisted)
                        try? modelContext.save()
                    }
                    saveStatus = "Saved"
                    saveError = nil
                } catch {
                    saveError = error.localizedDescription
                }
            }
            saveInProgress = false
        }
    }

    private func loadSettingsFromRust() async {
        loadState = .loading
        saveError = nil
        do {
            let result = try await bridge.loadSettings()
            if result.exists, let rustSettings = result.settings {
                upsertCachedSettings(rustSettings)
                loadState = .loaded
            } else {
                loadState = .missing
            }
        } catch {
            loadState = .failed(error.localizedDescription)
        }
    }

    private func createDefaultSettings() async {
        guard !createInProgress else { return }
        createInProgress = true
        saveError = nil
        defer { createInProgress = false }
        do {
            _ = try await bridge.createDefaultSettings()
            let reloaded = try await bridge.loadSettings()
            guard reloaded.exists, let rustSettings = reloaded.settings else {
                throw RustBridgeError.invalidResponse
            }
            upsertCachedSettings(rustSettings)
            loadState = .loaded
        } catch {
            saveError = error.localizedDescription
            loadState = .missing
        }
    }

    private func upsertCachedSettings(_ rustSettings: RustApplicationSettings) {
        let cache = appSettings ?? AppSettings()
        if appSettings == nil {
            modelContext.insert(cache)
        }
        cache.apply(rust: rustSettings)
        for duplicate in settings.dropFirst() {
            modelContext.delete(duplicate)
        }
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

struct SyncCredentialFormView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Bindable var settings: AppSettings

    @State private var accountLabel = ""
    @State private var newToken = ""
    @State private var message: String?
    @State private var configured = false
    @State private var confirmRemoval = false
    private let vault = KeychainCredentialVault()

    var body: some View {
        Form {
            Section("Status") {
                DetailRow(title: "Token", value: configured ? "Configured" : "Not configured")
                DetailRow(title: "Account", value: settings.syncAccountLabel ?? "")
            }

            Section("Credentials") {
                TextField("Account Label", text: $accountLabel)
                    .textInputAutocapitalization(.never)
                    .accessibilityHint("A non-secret label for the sync account.")
                SecureField("Sync Token", text: $newToken)
                    .textInputAutocapitalization(.never)
                    .textContentType(.password)
                    .accessibilityHint("Existing sync tokens are never displayed. Enter a new token to add or replace it.")
                Text("The sync token is saved in iOS Keychain. The Rust settings record stores only the account label.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Actions") {
                Button(configured ? "Replace Sync Token" : "Save Sync Token", action: saveToken)
                    .disabled(newToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Validate Stored Token", action: validateToken)
                    .disabled(!configured)
                Button("Remove Sync Token", role: .destructive) {
                    confirmRemoval = true
                }
                .disabled(!configured)
                if let message {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Sync Credentials")
        .onAppear(perform: loadState)
        .confirmationDialog("Remove sync token?", isPresented: $confirmRemoval, titleVisibility: .visible) {
            Button("Remove Sync Token", role: .destructive, action: removeToken)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The Keychain token will be deleted. Settings, QSOs, activations, and local sync data are preserved.")
        }
    }

    private func loadState() {
        accountLabel = settings.syncAccountLabel ?? ""
        configured = (try? vault.read(account: "sync_token", providerId: "sync")) != nil
    }

    private func saveToken() {
        do {
            try vault.save(secret: newToken, account: "sync_token", providerId: "sync")
            newToken = ""
            configured = true
            settings.syncAccountLabel = accountLabel.trimmingCharacters(in: .whitespacesAndNewlines)
            persistSettings()
            message = "Saved sync token in Keychain."
        } catch {
            message = error.localizedDescription
        }
    }

    private func validateToken() {
        configured = (try? vault.read(account: "sync_token", providerId: "sync")) != nil
        message = configured ? "Stored sync token is available in Keychain." : "Sync token is missing from Keychain."
    }

    private func removeToken() {
        try? vault.delete(account: "sync_token", providerId: "sync")
        configured = false
        newToken = ""
        message = "Removed sync token from Keychain."
    }

    private func persistSettings() {
        settings.updatedAt = Date()
        try? modelContext.save()
        Task {
            do {
                let result = try await bridge.saveSettings(settings.rustSettingsPayload())
                if let persisted = result.settings {
                    settings.apply(rust: persisted)
                    try? modelContext.save()
                }
            } catch {
                message = error.localizedDescription
            }
        }
    }
}

struct ProviderCredentialFormView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
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
                    persistSettings()
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
            persistSettings()
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
        persistSettings()
    }

    private func removeCredentials() {
        for field in definition.secureFields {
            try? vault.delete(account: field.id, providerId: definition.id)
        }
        settings.clearProviderCredentialMetadata(definition.id)
        persistSettings()
        fieldValues = [:]
        message = "Removed \(definition.displayName) credentials."
    }

    private func persistSettings() {
        settings.updatedAt = Date()
        try? modelContext.save()
        Task {
            do {
                let result = try await bridge.saveSettings(settings.rustSettingsPayload())
                if let persisted = result.settings {
                    settings.apply(rust: persisted)
                    try? modelContext.save()
                }
            } catch {
                message = error.localizedDescription
            }
        }
    }
}
