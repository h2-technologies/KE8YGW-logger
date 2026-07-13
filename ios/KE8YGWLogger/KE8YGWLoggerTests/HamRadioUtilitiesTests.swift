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

    func testMaidenheadNormalization() {
        XCTAssertEqual(HamRadioUtilities.normalizedMaidenhead(" en91 "), "EN91")
        XCTAssertEqual(HamRadioUtilities.normalizedMaidenhead("en91ab"), "EN91AB")
        XCTAssertNil(HamRadioUtilities.normalizedMaidenhead("ZZ99"))
        XCTAssertNil(HamRadioUtilities.normalizedMaidenhead("EN9"))
    }

    func testMaidenheadFromCoordinates() {
        XCTAssertEqual(HamRadioUtilities.maidenheadGrid(latitude: 41.0, longitude: -81.0, precision: 4), "EN91")
        XCTAssertEqual(HamRadioUtilities.maidenheadGrid(latitude: 41.0, longitude: -81.0, precision: 6)?.prefix(4), "EN91")
        XCTAssertNil(HamRadioUtilities.maidenheadGrid(latitude: 100, longitude: -81.0))
    }

    func testProviderValidationStatePersistsWithoutSecrets() {
        let settings = AppSettings()
        settings.setProviderEnabled("pota", enabled: false)
        XCTAssertFalse(settings.isProviderEnabled("pota"))

        settings.setProviderCredentialMetadata("pota", metadata: ["callsign": "KE8YGW", "api_key_configured": "true"])
        settings.setProviderValidationRecord("pota", record: ProviderValidationRecord(
            configured: true,
            validated: true,
            validatedAt: Date(),
            message: "ok"
        ))

        XCTAssertEqual(settings.providerCredentialMetadata("pota")["callsign"], "KE8YGW")
        XCTAssertEqual(settings.providerCredentialMetadata("pota")["api_key_configured"], "true")
        XCTAssertTrue(settings.providerValidationRecord("pota").validated)
    }

    func testActivationEligibilityRequiresEnabledValidatedProviderWhenOnline() {
        let settings = AppSettings()
        var eligibility = ActivationEligibility.evaluate(providerID: "pota", settings: settings, networkAvailable: true, validationTTLHours: 24)
        XCTAssertFalse(eligibility.canStart)
        XCTAssertEqual(eligibility.state, .credentialsMissing)

        settings.setProviderValidationRecord("pota", record: ProviderValidationRecord(
            configured: true,
            validated: true,
            validatedAt: Date(),
            message: "ok"
        ))
        eligibility = ActivationEligibility.evaluate(providerID: "pota", settings: settings, networkAvailable: true, validationTTLHours: 24)
        XCTAssertTrue(eligibility.canStart)
        XCTAssertEqual(eligibility.state, .providerValidated)

        settings.setProviderEnabled("pota", enabled: false)
        eligibility = ActivationEligibility.evaluate(providerID: "pota", settings: settings, networkAvailable: true, validationTTLHours: 24)
        XCTAssertFalse(eligibility.canStart)
        XCTAssertEqual(eligibility.state, .providerDisabled)
    }

    func testActivationEligibilityAllowsOnlyExplicitOfflineLocalStartWhenNetworkUnavailable() {
        let settings = AppSettings()
        settings.allowOfflineActivations = true
        let eligibility = ActivationEligibility.evaluate(providerID: "pota", settings: settings, networkAvailable: false, validationTTLHours: 24)
        XCTAssertTrue(eligibility.canStart)
        XCTAssertTrue(eligibility.offlineOnly)
        XCTAssertEqual(eligibility.state, .offlineLocalOnly)
    }

    func testNetClassificationOrdering() {
        let values: [NetTrafficClassification] = [.noTraffic, .routine, .emergency, .healthAndWelfare, .priority]
        XCTAssertEqual(values.sorted { $0.sortRank < $1.sortRank }, [.emergency, .priority, .routine, .healthAndWelfare, .noTraffic])
    }
}
