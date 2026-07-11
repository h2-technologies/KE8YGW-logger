import Foundation
import SwiftData

@Model
final class StationProfile: Identifiable {
    var id: UUID
    var displayName: String
    var profileType: String
    var operatorCallsign: String
    var stationCallsign: String
    var defaultGridSquare: String
    var defaultQTH: String
    var defaultState: String
    var defaultCountry: String
    var defaultPowerWatts: Double
    var notes: String
    var isActive: Bool
    var canonicalID: String
    var projectionVersion: Int
    var lastRustRevision: String
    var isTombstoned: Bool
    var projectionSource: String
    var projectionSchemaVersion: Int
    var lastProjectionRefreshAt: Date?
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        canonicalID: String = "",
        displayName: String = "Home Station",
        profileType: String = "home",
        operatorCallsign: String = "KE8YGW",
        stationCallsign: String = "KE8YGW",
        defaultGridSquare: String = "",
        defaultQTH: String = "",
        defaultState: String = "",
        defaultCountry: String = "United States",
        defaultPowerWatts: Double = 100,
        notes: String = "",
        isActive: Bool = true,
        projectionVersion: Int = 1,
        lastRustRevision: String = "",
        isTombstoned: Bool = false,
        projectionSource: String = "swiftdata_legacy",
        projectionSchemaVersion: Int = 1,
        lastProjectionRefreshAt: Date? = nil,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.canonicalID = canonicalID
        self.displayName = displayName
        self.profileType = profileType
        self.operatorCallsign = operatorCallsign
        self.stationCallsign = stationCallsign
        self.defaultGridSquare = defaultGridSquare
        self.defaultQTH = defaultQTH
        self.defaultState = defaultState
        self.defaultCountry = defaultCountry
        self.defaultPowerWatts = defaultPowerWatts
        self.notes = notes
        self.isActive = isActive
        self.projectionVersion = projectionVersion
        self.lastRustRevision = lastRustRevision
        self.isTombstoned = isTombstoned
        self.projectionSource = projectionSource
        self.projectionSchemaVersion = projectionSchemaVersion
        self.lastProjectionRefreshAt = lastProjectionRefreshAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
