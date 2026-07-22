import Foundation
import SwiftData

@Model
final class AppSettings: Identifiable {
    var id: UUID
    var settingsSchemaVersion: Int?
    var defaultBand: String
    var defaultMode: String
    var appearance: String
    var accentColorName: String
    var operatorCallsign: String
    var additionalCallsignsJSON: String?
    var operatorName: String?
    var operatorEmail: String?
    var stationCallsign: String
    var defaultStationProfileID: String?
    var defaultEquipmentProfileID: String?
    var maidenheadGrid: String?
    var manualGridOverrideEnabled: Bool?
    var useDeviceLocation: Bool?
    var lastGPSGrid: String?
    var lastGPSGridAt: Date?
    var lastLocationSource: String?
    var manualLocationName: String?
    var manualCounty: String?
    var manualState: String?
    var manualCountry: String?
    var serverURL: String?
    var syncDeviceName: String?
    var syncEndpointStyle: String?
    var syncIntervalMinutes: Int?
    var preferLANSync: Bool?
    var autoPushSync: Bool?
    var autoPullSync: Bool?
    var syncAccountLabel: String?
    var allowOfflineActivations: Bool?
    var validationTTLHours: Int?
    var activationNotesTemplate: String?
    var potaUploadEnabled: Bool?
    var sotaUploadEnabled: Bool?
    var netDefaultName: String?
    var netDefaultFrequencyMHz: String?
    var netDefaultMode: String?
    var sortNetRosterByTrafficPriority: Bool?
    var callsignLookupPreference: String?
    var mapDefaultLayer: String?
    var showQSOMapObjects: Bool?
    var showStationMapMarkers: Bool?
    var includeDiagnosticsInBackups: Bool?
    var providerStateJSON: String?
    var providerValidationJSON: String?
    var providerCredentialMetadataJSON: String?
    var qsoDraftJSON: String?
    var potaDraftJSON: String?
    var sotaDraftJSON: String?
    var netDraftJSON: String?
    var autoUppercaseCallsigns: Bool
    var askForLocationLater: Bool
    var backgroundSyncEnabled: Bool
    var providerNotificationsEnabled: Bool
    var shareDiagnosticsWithLogs: Bool
    var developerModeEnabled: Bool
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        settingsSchemaVersion: Int? = 4,
        defaultBand: String = "20m",
        defaultMode: String = "SSB",
        appearance: String = "system",
        accentColorName: String = "blue",
        operatorCallsign: String = "KE8YGW",
        additionalCallsignsJSON: String? = "[]",
        operatorName: String? = "",
        operatorEmail: String? = "",
        stationCallsign: String = "KE8YGW",
        defaultStationProfileID: String? = "",
        defaultEquipmentProfileID: String? = "",
        maidenheadGrid: String? = "EN91",
        manualGridOverrideEnabled: Bool? = false,
        useDeviceLocation: Bool? = true,
        lastGPSGrid: String? = "",
        lastGPSGridAt: Date? = nil,
        lastLocationSource: String? = MaidenheadLocationSource.stationDefault.rawValue,
        manualLocationName: String? = "",
        manualCounty: String? = "",
        manualState: String? = "",
        manualCountry: String? = "United States",
        serverURL: String? = "http://127.0.0.1:9740",
        syncDeviceName: String? = "KE8YGW Logger iOS",
        syncEndpointStyle: String? = "logbook_scoped",
        syncIntervalMinutes: Int? = 15,
        preferLANSync: Bool? = true,
        autoPushSync: Bool? = false,
        autoPullSync: Bool? = false,
        syncAccountLabel: String? = "",
        allowOfflineActivations: Bool? = true,
        validationTTLHours: Int? = 24,
        activationNotesTemplate: String? = "",
        potaUploadEnabled: Bool? = false,
        sotaUploadEnabled: Bool? = false,
        netDefaultName: String? = "Weekly Emergency Net",
        netDefaultFrequencyMHz: String? = "146.520",
        netDefaultMode: String? = "FM",
        sortNetRosterByTrafficPriority: Bool? = true,
        callsignLookupPreference: String? = "automatic",
        mapDefaultLayer: String? = "Stations",
        showQSOMapObjects: Bool? = true,
        showStationMapMarkers: Bool? = true,
        includeDiagnosticsInBackups: Bool? = false,
        providerStateJSON: String? = "",
        providerValidationJSON: String? = "",
        providerCredentialMetadataJSON: String? = "",
        qsoDraftJSON: String? = "",
        potaDraftJSON: String? = "",
        sotaDraftJSON: String? = "",
        netDraftJSON: String? = "",
        autoUppercaseCallsigns: Bool = true,
        askForLocationLater: Bool = false,
        backgroundSyncEnabled: Bool = true,
        providerNotificationsEnabled: Bool = true,
        shareDiagnosticsWithLogs: Bool = true,
        developerModeEnabled: Bool = false,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.settingsSchemaVersion = settingsSchemaVersion
        self.defaultBand = defaultBand
        self.defaultMode = defaultMode
        self.appearance = appearance
        self.accentColorName = accentColorName
        self.operatorCallsign = operatorCallsign
        self.additionalCallsignsJSON = additionalCallsignsJSON
        self.operatorName = operatorName
        self.operatorEmail = operatorEmail
        self.stationCallsign = stationCallsign
        self.defaultStationProfileID = defaultStationProfileID
        self.defaultEquipmentProfileID = defaultEquipmentProfileID
        self.maidenheadGrid = maidenheadGrid
        self.manualGridOverrideEnabled = manualGridOverrideEnabled
        self.useDeviceLocation = useDeviceLocation
        self.lastGPSGrid = lastGPSGrid
        self.lastGPSGridAt = lastGPSGridAt
        self.lastLocationSource = lastLocationSource
        self.manualLocationName = manualLocationName
        self.manualCounty = manualCounty
        self.manualState = manualState
        self.manualCountry = manualCountry
        self.serverURL = serverURL
        self.syncDeviceName = syncDeviceName
        self.syncEndpointStyle = syncEndpointStyle
        self.syncIntervalMinutes = syncIntervalMinutes
        self.preferLANSync = preferLANSync
        self.autoPushSync = autoPushSync
        self.autoPullSync = autoPullSync
        self.syncAccountLabel = syncAccountLabel
        self.allowOfflineActivations = allowOfflineActivations
        self.validationTTLHours = validationTTLHours
        self.activationNotesTemplate = activationNotesTemplate
        self.potaUploadEnabled = potaUploadEnabled
        self.sotaUploadEnabled = sotaUploadEnabled
        self.netDefaultName = netDefaultName
        self.netDefaultFrequencyMHz = netDefaultFrequencyMHz
        self.netDefaultMode = netDefaultMode
        self.sortNetRosterByTrafficPriority = sortNetRosterByTrafficPriority
        self.callsignLookupPreference = callsignLookupPreference
        self.mapDefaultLayer = mapDefaultLayer
        self.showQSOMapObjects = showQSOMapObjects
        self.showStationMapMarkers = showStationMapMarkers
        self.includeDiagnosticsInBackups = includeDiagnosticsInBackups
        self.providerStateJSON = providerStateJSON
        self.providerValidationJSON = providerValidationJSON
        self.providerCredentialMetadataJSON = providerCredentialMetadataJSON
        self.qsoDraftJSON = qsoDraftJSON
        self.potaDraftJSON = potaDraftJSON
        self.sotaDraftJSON = sotaDraftJSON
        self.netDraftJSON = netDraftJSON
        self.autoUppercaseCallsigns = autoUppercaseCallsigns
        self.askForLocationLater = askForLocationLater
        self.backgroundSyncEnabled = backgroundSyncEnabled
        self.providerNotificationsEnabled = providerNotificationsEnabled
        self.shareDiagnosticsWithLogs = shareDiagnosticsWithLogs
        self.developerModeEnabled = developerModeEnabled
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

extension AppSettings {
    static let currentSchemaVersion = 4

    var effectiveUseDeviceLocation: Bool {
        get { useDeviceLocation ?? true }
        set { useDeviceLocation = newValue }
    }

    var effectiveManualGridOverride: Bool {
        get { manualGridOverrideEnabled ?? false }
        set { manualGridOverrideEnabled = newValue }
    }

    var effectiveAllowOfflineActivations: Bool {
        get { allowOfflineActivations ?? true }
        set { allowOfflineActivations = newValue }
    }

    var effectiveValidationTTLHours: Int {
        get { validationTTLHours ?? 24 }
        set { validationTTLHours = max(1, newValue) }
    }

    func migrateIfNeeded() {
        if settingsSchemaVersion == AppSettings.currentSchemaVersion { return }
        settingsSchemaVersion = AppSettings.currentSchemaVersion
        additionalCallsignsJSON = additionalCallsignsJSON ?? "[]"
        operatorName = operatorName ?? ""
        operatorEmail = operatorEmail ?? ""
        defaultStationProfileID = defaultStationProfileID ?? ""
        defaultEquipmentProfileID = defaultEquipmentProfileID ?? ""
        maidenheadGrid = HamRadioUtilities.normalizedMaidenhead(maidenheadGrid ?? "") ?? maidenheadGrid ?? ""
        manualGridOverrideEnabled = manualGridOverrideEnabled ?? false
        useDeviceLocation = useDeviceLocation ?? true
        lastGPSGrid = lastGPSGrid ?? ""
        lastLocationSource = lastLocationSource ?? MaidenheadLocationSource.stationDefault.rawValue
        manualLocationName = manualLocationName ?? ""
        manualCounty = manualCounty ?? ""
        manualState = manualState ?? ""
        manualCountry = manualCountry ?? "United States"
        serverURL = serverURL ?? "http://127.0.0.1:9740"
        syncDeviceName = syncDeviceName ?? "KE8YGW Logger iOS"
        syncEndpointStyle = syncEndpointStyle ?? "logbook_scoped"
        syncIntervalMinutes = syncIntervalMinutes ?? 15
        preferLANSync = preferLANSync ?? true
        autoPushSync = autoPushSync ?? false
        autoPullSync = autoPullSync ?? false
        syncAccountLabel = syncAccountLabel ?? ""
        allowOfflineActivations = allowOfflineActivations ?? true
        validationTTLHours = validationTTLHours ?? 24
        activationNotesTemplate = activationNotesTemplate ?? ""
        potaUploadEnabled = potaUploadEnabled ?? false
        sotaUploadEnabled = sotaUploadEnabled ?? false
        netDefaultName = netDefaultName ?? "Weekly Emergency Net"
        netDefaultFrequencyMHz = netDefaultFrequencyMHz ?? "146.520"
        netDefaultMode = netDefaultMode ?? "FM"
        sortNetRosterByTrafficPriority = sortNetRosterByTrafficPriority ?? true
        callsignLookupPreference = callsignLookupPreference ?? "automatic"
        mapDefaultLayer = mapDefaultLayer ?? "Stations"
        showQSOMapObjects = showQSOMapObjects ?? true
        showStationMapMarkers = showStationMapMarkers ?? true
        includeDiagnosticsInBackups = includeDiagnosticsInBackups ?? false
        providerStateJSON = providerStateJSON ?? ""
        providerValidationJSON = providerValidationJSON ?? ""
        providerCredentialMetadataJSON = providerCredentialMetadataJSON ?? ""
        qsoDraftJSON = qsoDraftJSON ?? ""
        potaDraftJSON = potaDraftJSON ?? ""
        sotaDraftJSON = sotaDraftJSON ?? ""
        netDraftJSON = netDraftJSON ?? ""
        updatedAt = Date()
    }

    func isProviderEnabled(_ providerID: String) -> Bool {
        providerState()[providerID] ?? true
    }

    func setProviderEnabled(_ providerID: String, enabled: Bool) {
        var state = providerState()
        state[providerID] = enabled
        providerStateJSON = encodeDictionary(state)
        updatedAt = Date()
    }

    func providerValidationRecord(_ providerID: String) -> ProviderValidationRecord {
        providerValidationState()[providerID] ?? ProviderValidationRecord(
            configured: false,
            validated: false,
            validatedAt: nil,
            message: "Not configured"
        )
    }

    func setProviderValidationRecord(_ providerID: String, record: ProviderValidationRecord) {
        var state = providerValidationState()
        state[providerID] = record
        providerValidationJSON = encodeDictionary(state)
        updatedAt = Date()
    }

    func providerCredentialMetadata(_ providerID: String) -> [String: String] {
        providerCredentialMetadataState()[providerID] ?? [:]
    }

    func setProviderCredentialMetadata(_ providerID: String, metadata: [String: String]) {
        var state = providerCredentialMetadataState()
        state[providerID] = metadata
        providerCredentialMetadataJSON = encodeDictionary(state)
        updatedAt = Date()
    }

    func clearProviderCredentialMetadata(_ providerID: String) {
        var metadata = providerCredentialMetadataState()
        metadata.removeValue(forKey: providerID)
        providerCredentialMetadataJSON = encodeDictionary(metadata)
        setProviderValidationRecord(providerID, record: ProviderValidationRecord(
            configured: false,
            validated: false,
            validatedAt: nil,
            message: "Credentials removed"
        ))
        updatedAt = Date()
    }

    var additionalCallsigns: [String] {
        decodeDictionaryArray(additionalCallsignsJSON)
    }

    func setAdditionalCallsigns(_ callsigns: [String]) {
        additionalCallsignsJSON = encodeArray(
            callsigns
                .map { HamRadioUtilities.normalizeCallsign($0) }
                .filter { !$0.isEmpty }
        )
        updatedAt = Date()
    }

    func apply(rust settings: RustApplicationSettings) {
        settingsSchemaVersion = settings.schemaVersion
        appearance = settings.display.appearance
        accentColorName = settings.display.accentColorName
        operatorCallsign = settings.operator.primaryCallsign
        setAdditionalCallsigns(settings.operator.additionalCallsigns)
        operatorName = settings.operator.operatorName
        operatorEmail = settings.operator.operatorEmail
        stationCallsign = settings.operator.stationCallsign
        defaultStationProfileID = settings.operator.defaultStationProfileId
        defaultEquipmentProfileID = settings.operator.defaultEquipmentProfileId
        maidenheadGrid = settings.location.manualMaidenheadGrid
        manualGridOverrideEnabled = settings.location.manualGridOverrideEnabled
        useDeviceLocation = settings.location.useDeviceLocation
        lastGPSGrid = settings.location.lastGpsGrid
        lastLocationSource = settings.location.lastLocationSource
        manualLocationName = settings.location.manualLocationName
        manualCounty = settings.location.manualCounty
        manualState = settings.location.manualState
        manualCountry = settings.location.manualCountry
        serverURL = settings.sync.syncServerUrl
        syncDeviceName = settings.sync.deviceName
        syncEndpointStyle = settings.sync.syncEndpointStyle ?? "logbook_scoped"
        syncIntervalMinutes = settings.sync.syncIntervalMinutes
        preferLANSync = settings.sync.preferLanSync
        autoPushSync = settings.sync.autoPushEnabled
        autoPullSync = settings.sync.autoPullEnabled
        backgroundSyncEnabled = settings.sync.backgroundSyncEnabled
        syncAccountLabel = settings.sync.accountLabel
        defaultBand = settings.logging.defaultBand
        defaultMode = settings.logging.defaultMode
        autoUppercaseCallsigns = settings.logging.autoUppercaseCallsigns
        askForLocationLater = settings.logging.askForLocationLater
        callsignLookupPreference = settings.logging.callsignLookupPreference
        allowOfflineActivations = settings.activation.allowOfflineActivations
        validationTTLHours = settings.activation.validationTtlHours
        activationNotesTemplate = settings.activation.notesTemplate
        potaUploadEnabled = settings.activation.potaUploadEnabled
        sotaUploadEnabled = settings.activation.sotaUploadEnabled
        netDefaultName = settings.netControl.defaultName
        netDefaultFrequencyMHz = settings.netControl.defaultFrequencyMhz
        netDefaultMode = settings.netControl.defaultMode
        sortNetRosterByTrafficPriority = settings.netControl.sortRosterByTrafficPriority
        mapDefaultLayer = settings.display.mapDefaultLayer
        showQSOMapObjects = settings.display.showQsoMapObjects
        showStationMapMarkers = settings.display.showStationMapMarkers
        includeDiagnosticsInBackups = settings.backup.includeDiagnosticsByDefault
        providerNotificationsEnabled = settings.privacy.providerNotificationsEnabled
        shareDiagnosticsWithLogs = settings.diagnostics.shareDiagnosticsWithLogs
        developerModeEnabled = settings.developer.developerModeEnabled
        providerStateJSON = encodeDictionary(settings.providers.enabled)
        providerCredentialMetadataJSON = encodeDictionary(settings.providers.credentialMetadata)
        providerValidationJSON = encodeDictionary(settings.providers.validation.mapValues {
            ProviderValidationRecord(
                configured: $0.configured,
                validated: $0.validated,
                validatedAt: $0.validatedAt.flatMap { ISO8601DateFormatter().date(from: $0) },
                message: $0.message
            )
        })
        createdAt = ISO8601DateFormatter().date(from: settings.createdAt) ?? createdAt
        updatedAt = ISO8601DateFormatter().date(from: settings.updatedAt) ?? Date()
    }

    func rustSettingsPayload() -> RustApplicationSettings {
        let formatter = ISO8601DateFormatter()
        let providerValidation = providerValidationState().mapValues {
            RustProviderValidationSettings(
                configured: $0.configured,
                validated: $0.validated,
                validatedAt: $0.validatedAt.map { formatter.string(from: $0) },
                message: $0.message
            )
        }
        return RustApplicationSettings(
            schemaVersion: settingsSchemaVersion ?? AppSettings.currentSchemaVersion,
            operator: RustOperatorIdentitySettings(
                primaryCallsign: operatorCallsign,
                additionalCallsigns: additionalCallsigns,
                operatorName: operatorName,
                operatorEmail: operatorEmail,
                stationCallsign: stationCallsign,
                defaultStationProfileId: defaultStationProfileID,
                defaultEquipmentProfileId: defaultEquipmentProfileID
            ),
            location: RustLocationSettings(
                useDeviceLocation: effectiveUseDeviceLocation,
                manualGridOverrideEnabled: effectiveManualGridOverride,
                manualMaidenheadGrid: maidenheadGrid,
                lastGpsGrid: lastGPSGrid,
                lastLocationSource: lastLocationSource,
                manualLocationName: manualLocationName,
                manualCounty: manualCounty,
                manualState: manualState,
                manualCountry: manualCountry
            ),
            providers: RustProviderSettings(
                enabled: providerState(),
                credentialMetadata: providerCredentialMetadataState(),
                validation: providerValidation
            ),
            sync: RustSyncSettings(
                syncServerUrl: serverURL ?? "",
                deviceName: syncDeviceName ?? "",
                syncEndpointStyle: syncEndpointStyle ?? "logbook_scoped",
                preferLanSync: preferLANSync ?? true,
                autoPushEnabled: autoPushSync ?? false,
                autoPullEnabled: autoPullSync ?? false,
                syncIntervalMinutes: syncIntervalMinutes ?? 15,
                backgroundSyncEnabled: backgroundSyncEnabled,
                accountLabel: syncAccountLabel
            ),
            logging: RustLoggingSettings(
                defaultBand: defaultBand,
                defaultMode: defaultMode,
                autoUppercaseCallsigns: autoUppercaseCallsigns,
                askForLocationLater: askForLocationLater,
                callsignLookupPreference: callsignLookupPreference ?? "automatic"
            ),
            activation: RustActivationSettings(
                allowOfflineActivations: allowOfflineActivations ?? true,
                validationTtlHours: validationTTLHours ?? 24,
                notesTemplate: activationNotesTemplate,
                potaUploadEnabled: potaUploadEnabled ?? false,
                sotaUploadEnabled: sotaUploadEnabled ?? false
            ),
            netControl: RustNetControlSettings(
                defaultName: netDefaultName,
                defaultFrequencyMhz: netDefaultFrequencyMHz,
                defaultMode: netDefaultMode ?? "FM",
                sortRosterByTrafficPriority: sortNetRosterByTrafficPriority ?? true
            ),
            display: RustDisplaySettings(
                appearance: appearance,
                accentColorName: accentColorName,
                mapDefaultLayer: mapDefaultLayer ?? "Stations",
                showQsoMapObjects: showQSOMapObjects ?? true,
                showStationMapMarkers: showStationMapMarkers ?? true
            ),
            backup: RustBackupSettings(includeDiagnosticsByDefault: includeDiagnosticsInBackups ?? false),
            privacy: RustPrivacySettings(providerNotificationsEnabled: providerNotificationsEnabled),
            diagnostics: RustDiagnosticsSettings(shareDiagnosticsWithLogs: shareDiagnosticsWithLogs),
            developer: RustDeveloperSettings(developerModeEnabled: developerModeEnabled),
            createdAt: formatter.string(from: createdAt),
            updatedAt: formatter.string(from: updatedAt)
        )
    }

    private func providerState() -> [String: Bool] {
        decodeDictionary(providerStateJSON)
    }

    private func providerValidationState() -> [String: ProviderValidationRecord] {
        decodeDictionary(providerValidationJSON)
    }

    private func providerCredentialMetadataState() -> [String: [String: String]] {
        decodeDictionary(providerCredentialMetadataJSON)
    }

    private func decodeDictionary<T: Decodable>(_ json: String?) -> [String: T] {
        guard let json, !json.isEmpty, let data = json.data(using: .utf8) else { return [:] }
        return (try? JSONDecoder().decode([String: T].self, from: data)) ?? [:]
    }

    private func encodeDictionary<T: Encodable>(_ dictionary: [String: T]) -> String {
        guard let data = try? JSONEncoder().encode(dictionary) else { return "{}" }
        return String(data: data, encoding: .utf8) ?? "{}"
    }

    private func decodeDictionaryArray(_ json: String?) -> [String] {
        guard let json, !json.isEmpty, let data = json.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([String].self, from: data)) ?? []
    }

    private func encodeArray<T: Encodable>(_ array: [T]) -> String {
        guard let data = try? JSONEncoder().encode(array) else { return "[]" }
        return String(data: data, encoding: .utf8) ?? "[]"
    }
}
