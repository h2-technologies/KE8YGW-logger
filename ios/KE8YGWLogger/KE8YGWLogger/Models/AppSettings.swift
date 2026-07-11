import Foundation
import SwiftData

@Model
final class AppSettings: Identifiable {
    var id: UUID
    var defaultBand: String
    var defaultMode: String
    var appearance: String
    var accentColorName: String
    var operatorCallsign: String
    var stationCallsign: String
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
        defaultBand: String = "20m",
        defaultMode: String = "SSB",
        appearance: String = "system",
        accentColorName: String = "blue",
        operatorCallsign: String = "KE8YGW",
        stationCallsign: String = "KE8YGW",
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
        self.defaultBand = defaultBand
        self.defaultMode = defaultMode
        self.appearance = appearance
        self.accentColorName = accentColorName
        self.operatorCallsign = operatorCallsign
        self.stationCallsign = stationCallsign
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
