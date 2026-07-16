import Foundation
import SwiftData

@Model
final class QSO: Identifiable {
    var id: UUID
    var callsign: String
    var contactDate: Date
    var band: String
    var mode: String
    var submode: String
    var frequencyMHz: Double
    var rstSent: String
    var rstReceived: String
    var powerWatts: Double
    var operatorCallsign: String
    var stationCallsign: String
    var stationProfileID: String
    var equipmentSummary: String
    var gridSquare: String
    var county: String
    var name: String
    var qth: String
    var state: String
    var country: String
    var qsoKind: String
    var contestExchange: String
    var satelliteName: String
    var potaReferences: String
    var sotaReferences: String
    var uploadStatus: String
    var syncStatus: String
    var canonicalID: String
    var clientOperationID: String
    var projectionVersion: Int
    var lastRustRevision: String
    var isTombstoned: Bool
    var projectionSource: String
    var projectionSchemaVersion: Int
    var lastProjectionRefreshAt: Date?
    var rustEventID: String
    var lastEventHash: String
    var notes: String
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        callsign: String,
        contactDate: Date = Date(),
        band: String,
        mode: String,
        submode: String = "",
        frequencyMHz: Double,
        rstSent: String,
        rstReceived: String,
        powerWatts: Double = 0,
        operatorCallsign: String,
        stationCallsign: String,
        stationProfileID: String = "",
        equipmentSummary: String = "",
        gridSquare: String = "",
        county: String = "",
        name: String = "",
        qth: String = "",
        state: String = "",
        country: String = "",
        qsoKind: String = "voice",
        contestExchange: String = "",
        satelliteName: String = "",
        potaReferences: String = "",
        sotaReferences: String = "",
        uploadStatus: String = "pending",
        syncStatus: String = "local",
        canonicalID: String = "",
        clientOperationID: String = "",
        projectionVersion: Int = 1,
        lastRustRevision: String = "",
        isTombstoned: Bool = false,
        projectionSource: String = "swiftdata_legacy",
        projectionSchemaVersion: Int = 1,
        lastProjectionRefreshAt: Date? = nil,
        rustEventID: String = "",
        lastEventHash: String = "",
        notes: String = "",
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.callsign = callsign
        self.contactDate = contactDate
        self.band = band
        self.mode = mode
        self.submode = submode
        self.frequencyMHz = frequencyMHz
        self.rstSent = rstSent
        self.rstReceived = rstReceived
        self.powerWatts = powerWatts
        self.operatorCallsign = operatorCallsign
        self.stationCallsign = stationCallsign
        self.stationProfileID = stationProfileID
        self.equipmentSummary = equipmentSummary
        self.gridSquare = gridSquare
        self.county = county
        self.name = name
        self.qth = qth
        self.state = state
        self.country = country
        self.qsoKind = qsoKind
        self.contestExchange = contestExchange
        self.satelliteName = satelliteName
        self.potaReferences = potaReferences
        self.sotaReferences = sotaReferences
        self.uploadStatus = uploadStatus
        self.syncStatus = syncStatus
        self.canonicalID = canonicalID
        self.clientOperationID = clientOperationID
        self.projectionVersion = projectionVersion
        self.lastRustRevision = lastRustRevision
        self.isTombstoned = isTombstoned
        self.projectionSource = projectionSource
        self.projectionSchemaVersion = projectionSchemaVersion
        self.lastProjectionRefreshAt = lastProjectionRefreshAt
        self.rustEventID = rustEventID
        self.lastEventHash = lastEventHash
        self.notes = notes
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
