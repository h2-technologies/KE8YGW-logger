import Foundation
import SwiftData

@Model
final class StationEquipment: Identifiable {
    var id: UUID
    var equipmentType: String
    var displayName: String
    var manufacturer: String
    var model: String
    var serialNumber: String
    var capabilities: String
    var status: String
    var notes: String
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
        equipmentType: String = "radio",
        displayName: String = "",
        manufacturer: String = "",
        model: String = "",
        serialNumber: String = "",
        capabilities: String = "",
        status: String = "active",
        notes: String = "",
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
        self.equipmentType = equipmentType
        self.displayName = displayName
        self.manufacturer = manufacturer
        self.model = model
        self.serialNumber = serialNumber
        self.capabilities = capabilities
        self.status = status
        self.notes = notes
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
