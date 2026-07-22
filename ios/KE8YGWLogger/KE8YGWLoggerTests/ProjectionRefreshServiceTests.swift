import SwiftData
import XCTest
@testable import KE8YGWLogger

final class ProjectionRefreshServiceTests: XCTestCase {
    @MainActor
    func testRebuildQSOProjectionUsesRustRecordsAndTombstonesStaleRows() throws {
        let container = try ModelContainer(
            for: QSO.self,
            configurations: ModelConfiguration(isStoredInMemoryOnly: true)
        )
        let modelContext = ModelContext(container)
        let stale = QSO(
            callsign: "K0OLD",
            band: "40m",
            mode: "SSB",
            frequencyMHz: 7.2,
            rstSent: "59",
            rstReceived: "59",
            operatorCallsign: "KE8YGW",
            stationCallsign: "KE8YGW",
            syncStatus: "synced",
            canonicalID: "stale-qso",
            projectionSource: "rust"
        )
        modelContext.insert(stale)
        try modelContext.save()

        let record = RustQSORecord(
            qsoId: "00000000-0000-4000-8000-000000000028",
            payload: RustQSOPayload(
                contactedCallsign: "W1AW",
                stationCallsign: "KE8YGW",
                operatorCallsign: "KE8YGW",
                startedAt: "2026-07-10T12:00:00Z",
                mode: "SSB",
                band: "20m",
                frequencyHz: 14_250_000,
                rstSent: "59",
                rstReceived: "57",
                powerWatts: 100,
                notes: "remote pull"
            ),
            deleted: false,
            lastEventHash: "remote-event-hash",
            projectionSource: "rust",
            schemaVersion: 1
        )

        let refreshed = try ProjectionRefreshService.rebuildQSOProjection(
            from: [record],
            existing: [stale],
            modelContext: modelContext,
            syncStatus: "synced"
        )
        let fetched = try modelContext.fetch(FetchDescriptor<QSO>())
        let pulled = try XCTUnwrap(fetched.first { $0.canonicalID == record.qsoId })
        let tombstoned = try XCTUnwrap(fetched.first { $0.canonicalID == "stale-qso" })

        XCTAssertEqual(refreshed, 2)
        XCTAssertEqual(pulled.callsign, "W1AW")
        XCTAssertEqual(pulled.frequencyMHz, 14.25)
        XCTAssertEqual(pulled.syncStatus, "synced")
        XCTAssertEqual(pulled.projectionSource, "rust")
        XCTAssertEqual(pulled.lastRustRevision, "remote-event-hash")
        XCTAssertFalse(pulled.isTombstoned)
        XCTAssertTrue(tombstoned.isTombstoned)
        XCTAssertEqual(tombstoned.syncStatus, "deleted")
    }
}
