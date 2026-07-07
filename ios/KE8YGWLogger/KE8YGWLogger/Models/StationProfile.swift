import Foundation
import SwiftData

@Model
final class StationProfile: Identifiable {
    var id: UUID
    var operatorCallsign: String
    var stationCallsign: String
    var defaultGridSquare: String
    var defaultQTH: String
    var defaultState: String
    var defaultCountry: String
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        operatorCallsign: String = "KE8YGW",
        stationCallsign: String = "KE8YGW",
        defaultGridSquare: String = "",
        defaultQTH: String = "",
        defaultState: String = "",
        defaultCountry: String = "United States",
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.operatorCallsign = operatorCallsign
        self.stationCallsign = stationCallsign
        self.defaultGridSquare = defaultGridSquare
        self.defaultQTH = defaultQTH
        self.defaultState = defaultState
        self.defaultCountry = defaultCountry
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
