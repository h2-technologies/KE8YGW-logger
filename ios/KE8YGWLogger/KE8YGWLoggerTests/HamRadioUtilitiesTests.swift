import XCTest
@testable import KE8YGWLogger

final class HamRadioUtilitiesTests: XCTestCase {
    func testCallsignNormalization() {
        XCTAssertEqual(HamRadioUtilities.normalizeCallsign(" ke8ygw "), "KE8YGW")
    }

    func testCallsignValidation() {
        XCTAssertTrue(HamRadioUtilities.isValidCallsign("KE8YGW"))
        XCTAssertTrue(HamRadioUtilities.isValidCallsign("K1ABC"))
        XCTAssertFalse(HamRadioUtilities.isValidCallsign(""))
        XCTAssertFalse(HamRadioUtilities.isValidCallsign("NOT A CALL"))
    }

    func testRSTDefaults() {
        XCTAssertEqual(HamRadioUtilities.defaultRST(for: "SSB"), "59")
        XCTAssertEqual(HamRadioUtilities.defaultRST(for: "FM"), "59")
        XCTAssertEqual(HamRadioUtilities.defaultRST(for: "CW"), "599")
        XCTAssertEqual(HamRadioUtilities.defaultRST(for: "FT8"), "599")
    }

    func testBandFromFrequency() {
        XCTAssertEqual(HamRadioUtilities.bandFromFrequencyMHz(14.250), "20m")
        XCTAssertEqual(HamRadioUtilities.bandFromFrequencyMHz(146.520), "2m")
        XCTAssertNil(HamRadioUtilities.bandFromFrequencyMHz(88.1))
    }
}
