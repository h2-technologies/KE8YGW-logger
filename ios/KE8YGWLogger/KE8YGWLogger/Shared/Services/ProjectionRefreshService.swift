import Foundation
import SwiftData

enum ProjectionRefreshService {
    static func upsertQSO(
        from record: RustQSORecord,
        event: RustOfficialEvent,
        operationID: String,
        existing qsos: [QSO],
        modelContext: ModelContext
    ) throws -> QSO {
        let target = qsos.first {
            $0.canonicalID == record.qsoId || $0.id.uuidString.caseInsensitiveCompare(record.qsoId) == .orderedSame
        } ?? QSO(
            id: UUID(uuidString: record.qsoId) ?? UUID(),
            callsign: record.payload.contactedCallsign ?? "",
            band: record.payload.band ?? "",
            mode: record.payload.mode ?? "",
            frequencyMHz: frequencyMHz(from: record.payload),
            rstSent: record.payload.rstSent ?? "",
            rstReceived: record.payload.rstReceived ?? "",
            operatorCallsign: record.payload.operatorCallsign ?? "",
            stationCallsign: record.payload.stationCallsign ?? ""
        )

        if !qsos.contains(where: { $0 === target }) {
            modelContext.insert(target)
        }

        target.canonicalID = record.qsoId
        target.clientOperationID = operationID
        target.callsign = record.payload.contactedCallsign ?? target.callsign
        target.contactDate = parsedDate(record.payload.startedAt) ?? target.contactDate
        target.band = record.payload.band ?? target.band
        target.mode = record.payload.mode ?? target.mode
        target.submode = record.payload.submode ?? target.submode
        target.frequencyMHz = frequencyMHz(from: record.payload)
        target.rstSent = record.payload.rstSent ?? target.rstSent
        target.rstReceived = record.payload.rstReceived ?? target.rstReceived
        target.powerWatts = record.payload.powerWatts ?? target.powerWatts
        target.operatorCallsign = record.payload.operatorCallsign ?? target.operatorCallsign
        target.stationCallsign = record.payload.stationCallsign ?? target.stationCallsign
        target.stationProfileID = record.payload.stationProfileId ?? target.stationProfileID
        target.equipmentSummary = record.payload.equipmentSummary ?? target.equipmentSummary
        target.gridSquare = record.payload.grid ?? target.gridSquare
        target.county = record.payload.county ?? target.county
        target.name = record.payload.name ?? target.name
        target.qth = record.payload.qth ?? target.qth
        target.state = record.payload.state ?? target.state
        target.country = record.payload.country ?? target.country
        target.qsoKind = record.payload.qsoKind ?? target.qsoKind
        target.contestExchange = record.payload.contestExchange ?? target.contestExchange
        target.satelliteName = record.payload.satelliteName ?? target.satelliteName
        target.potaReferences = record.payload.potaReferences ?? target.potaReferences
        target.sotaReferences = record.payload.sotaReferences ?? target.sotaReferences
        target.notes = record.payload.notes ?? target.notes
        target.uploadStatus = "pending"
        target.syncStatus = record.deleted ? "deleted" : "accepted_local"
        target.rustEventID = event.eventId
        target.lastEventHash = record.lastEventHash
        target.lastRustRevision = event.eventHash
        target.projectionVersion = record.schemaVersion ?? 1
        target.projectionSource = record.projectionSource ?? "rust"
        target.projectionSchemaVersion = record.schemaVersion ?? 1
        target.isTombstoned = record.deleted
        target.lastProjectionRefreshAt = Date()
        target.updatedAt = Date()

        try modelContext.save()
        return target
    }

    static func rebuildStationBook(
        from snapshot: StationBookSnapshot,
        profiles existingProfiles: [StationProfile],
        equipment existingEquipment: [StationEquipment],
        modelContext: ModelContext
    ) throws {
        let activeProfileID = snapshot.activeProfileId
        let profileIDs = Set(snapshot.profiles.map(\.stationProfileId))
        let equipmentIDs = Set(snapshot.equipment.map(\.equipmentId))

        for profileSnapshot in snapshot.profiles {
            let target = existingProfiles.first {
                $0.canonicalID == profileSnapshot.stationProfileId || $0.id.uuidString.caseInsensitiveCompare(profileSnapshot.stationProfileId) == .orderedSame
            } ?? StationProfile(
                id: UUID(uuidString: profileSnapshot.stationProfileId) ?? UUID(),
                canonicalID: profileSnapshot.stationProfileId,
                displayName: profileSnapshot.displayName,
                profileType: profileSnapshot.tags?.first ?? "home",
                operatorCallsign: profileSnapshot.operatorCallsign ?? profileSnapshot.stationCallsign,
                stationCallsign: profileSnapshot.stationCallsign,
                defaultGridSquare: profileSnapshot.defaultGrid ?? "",
                defaultQTH: profileSnapshot.defaultQth ?? "",
                defaultPowerWatts: Double(profileSnapshot.defaultPowerWatts ?? 0),
                isActive: profileSnapshot.stationProfileId == activeProfileID,
                projectionSource: "rust"
            )
            if !existingProfiles.contains(where: { $0 === target }) {
                modelContext.insert(target)
            }
            target.canonicalID = profileSnapshot.stationProfileId
            target.displayName = profileSnapshot.displayName
            target.profileType = profileSnapshot.tags?.first ?? target.profileType
            target.operatorCallsign = profileSnapshot.operatorCallsign ?? target.operatorCallsign
            target.stationCallsign = profileSnapshot.stationCallsign
            target.defaultGridSquare = profileSnapshot.defaultGrid ?? target.defaultGridSquare
            target.defaultQTH = profileSnapshot.defaultQth ?? target.defaultQTH
            target.defaultPowerWatts = Double(profileSnapshot.defaultPowerWatts ?? Int(target.defaultPowerWatts))
            target.isActive = profileSnapshot.stationProfileId == activeProfileID || profileSnapshot.active == true
            target.isTombstoned = false
            target.projectionSource = "rust"
            target.projectionSchemaVersion = 1
            target.lastRustRevision = activeProfileID ?? ""
            target.lastProjectionRefreshAt = Date()
            target.updatedAt = Date()
        }

        for profile in existingProfiles where profile.projectionSource == "rust" && !profileIDs.contains(profile.canonicalID) {
            profile.isTombstoned = true
            profile.isActive = false
            profile.lastProjectionRefreshAt = Date()
        }

        for equipmentSnapshot in snapshot.equipment {
            let target = existingEquipment.first {
                $0.canonicalID == equipmentSnapshot.equipmentId || $0.id.uuidString.caseInsensitiveCompare(equipmentSnapshot.equipmentId) == .orderedSame
            } ?? StationEquipment(
                id: UUID(uuidString: equipmentSnapshot.equipmentId) ?? UUID(),
                canonicalID: equipmentSnapshot.equipmentId,
                equipmentType: equipmentSnapshot.equipmentType,
                displayName: equipmentSnapshot.displayName,
                manufacturer: equipmentSnapshot.manufacturer ?? "",
                model: equipmentSnapshot.model ?? "",
                capabilities: equipmentSnapshot.capabilities?.joined(separator: ", ") ?? "",
                status: equipmentSnapshot.status ?? "active",
                projectionSource: "rust"
            )
            if !existingEquipment.contains(where: { $0 === target }) {
                modelContext.insert(target)
            }
            target.canonicalID = equipmentSnapshot.equipmentId
            target.equipmentType = equipmentSnapshot.equipmentType
            target.displayName = equipmentSnapshot.displayName
            target.manufacturer = equipmentSnapshot.manufacturer ?? target.manufacturer
            target.model = equipmentSnapshot.model ?? target.model
            target.capabilities = equipmentSnapshot.capabilities?.joined(separator: ", ") ?? target.capabilities
            target.status = equipmentSnapshot.status ?? target.status
            target.isTombstoned = false
            target.projectionSource = "rust"
            target.projectionSchemaVersion = 1
            target.lastProjectionRefreshAt = Date()
            target.updatedAt = Date()
        }

        for item in existingEquipment where item.projectionSource == "rust" && !equipmentIDs.contains(item.canonicalID) {
            item.isTombstoned = true
            item.lastProjectionRefreshAt = Date()
        }

        try modelContext.save()
    }

    private static func frequencyMHz(from payload: RustQSOPayload) -> Double {
        if let frequencyMhz = payload.frequencyMhz {
            return frequencyMhz
        }
        if let frequencyHz = payload.frequencyHz {
            return Double(frequencyHz) / 1_000_000
        }
        return 0
    }

    private static func parsedDate(_ value: String?) -> Date? {
        guard let value else { return nil }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) {
            return date
        }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }
}
