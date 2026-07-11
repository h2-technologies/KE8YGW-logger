import XCTest
@testable import KE8YGWLogger

final class ExportServiceTests: XCTestCase {
    private func sampleQSO() -> QSO {
        QSO(
            callsign: "K1ABC",
            contactDate: Date(timeIntervalSince1970: 1_704_067_200),
            band: "20m",
            mode: "SSB",
            frequencyMHz: 14.250,
            rstSent: "59",
            rstReceived: "57",
            operatorCallsign: "KE8YGW",
            stationCallsign: "KE8YGW",
            gridSquare: "EN91",
            name: "Alex",
            qth: "Cleveland, OH",
            state: "OH",
            country: "United States",
            notes: "Portable, clean signal"
        )
    }

    func testADIFExportFormatting() {
        let adif = LogExportService.adif(for: [sampleQSO()])

        XCTAssertTrue(adif.contains("<PROGRAMID:12>KE8YGWLogger"))
        XCTAssertTrue(adif.contains("<CALL:5>K1ABC"))
        XCTAssertTrue(adif.contains("<BAND:3>20m"))
        XCTAssertTrue(adif.contains("<MODE:3>SSB"))
        XCTAssertTrue(adif.contains("<FREQ:9>14.250000"))
        XCTAssertTrue(adif.contains("<EOR>"))
    }

    func testCSVExportFormatting() {
        let csv = LogExportService.csv(for: [sampleQSO()])

        XCTAssertTrue(csv.hasPrefix("Callsign,DateTimeUTC,Type,Band,Mode"))
        XCTAssertTrue(csv.contains("K1ABC"))
        XCTAssertTrue(csv.contains("\"Cleveland, OH\""))
        XCTAssertTrue(csv.contains("\"Portable, clean signal\""))
    }

    func testADIFDateFormattingStability() {
        let date = Date(timeIntervalSince1970: 1_704_067_200)

        XCTAssertEqual(HamDateFormatters.adifDate.string(from: date), "20240101")
        XCTAssertEqual(HamDateFormatters.adifTime.string(from: date), "000000")
    }
}
