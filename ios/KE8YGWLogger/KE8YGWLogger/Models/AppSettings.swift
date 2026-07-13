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
    var syncIntervalMinutes: Int?
    var preferLANSync: Bool?
    var autoPushSync: Bool?
    var autoPullSync: Bool?
    var allowOfflineActivations: Bool?
    var validationTTLHours: Int?
    var activationNotesTemplate: String?
    var netDefaultName: String?
    var netDefaultFrequencyMHz: String?
    var netDefaultMode: String?
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
        settingsSchemaVersion: Int? = 2,
        defaultBand: String = "20m",
        defaultMode: String = "SSB",
        appearance: String = "system",
        accentColorName: String = "blue",
        operatorCallsign: String = "KE8YGW",
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
        syncIntervalMinutes: Int? = 15,
        preferLANSync: Bool? = true,
        autoPushSync: Bool? = false,
        autoPullSync: Bool? = false,
        allowOfflineActivations: Bool? = true,
        validationTTLHours: Int? = 24,
        activationNotesTemplate: String? = "",
        netDefaultName: String? = "Weekly Emergency Net",
        netDefaultFrequencyMHz: String? = "146.520",
        netDefaultMode: String? = "FM",
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
        self.syncIntervalMinutes = syncIntervalMinutes
        self.preferLANSync = preferLANSync
        self.autoPushSync = autoPushSync
        self.autoPullSync = autoPullSync
        self.allowOfflineActivations = allowOfflineActivations
        self.validationTTLHours = validationTTLHours
        self.activationNotesTemplate = activationNotesTemplate
        self.netDefaultName = netDefaultName
        self.netDefaultFrequencyMHz = netDefaultFrequencyMHz
        self.netDefaultMode = netDefaultMode
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
    static let currentSchemaVersion = 2

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
        syncIntervalMinutes = syncIntervalMinutes ?? 15
        preferLANSync = preferLANSync ?? true
        autoPushSync = autoPushSync ?? false
        autoPullSync = autoPullSync ?? false
        allowOfflineActivations = allowOfflineActivations ?? true
        validationTTLHours = validationTTLHours ?? 24
        activationNotesTemplate = activationNotesTemplate ?? ""
        netDefaultName = netDefaultName ?? "Weekly Emergency Net"
        netDefaultFrequencyMHz = netDefaultFrequencyMHz ?? "146.520"
        netDefaultMode = netDefaultMode ?? "FM"
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
}
