import Foundation
import SwiftData

@Model
final class QSO: Identifiable {
    var id: UUID
    var callsign: String
    var contactDate: Date
    var band: String
    var mode: String
    var frequencyMHz: Double
    var rstSent: String
    var rstReceived: String
    var operatorCallsign: String
    var stationCallsign: String
    var gridSquare: String
    var name: String
    var qth: String
    var state: String
    var country: String
    var notes: String
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        callsign: String,
        contactDate: Date = Date(),
        band: String,
        mode: String,
        frequencyMHz: Double,
        rstSent: String,
        rstReceived: String,
        operatorCallsign: String,
        stationCallsign: String,
        gridSquare: String = "",
        name: String = "",
        qth: String = "",
        state: String = "",
        country: String = "",
        notes: String = "",
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.callsign = callsign
        self.contactDate = contactDate
        self.band = band
        self.mode = mode
        self.frequencyMHz = frequencyMHz
        self.rstSent = rstSent
        self.rstReceived = rstReceived
        self.operatorCallsign = operatorCallsign
        self.stationCallsign = stationCallsign
        self.gridSquare = gridSquare
        self.name = name
        self.qth = qth
        self.state = state
        self.country = country
        self.notes = notes
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
