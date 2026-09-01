import SwiftUI
import XCTest
@testable import KE8YGWLogger

final class DashboardLayoutTests: XCTestCase {
    /// The raw values are the identifiers persisted in the shared Rust settings
    /// (`display.mobile_dashboard_layout`). Renaming one silently strands every
    /// operator who had it selected, so pin them here.
    func testLayoutIdentifiersMatchTheSharedSettingsVocabulary() {
        XCTAssertEqual(
            DashboardLayout.allCases.map(\.rawValue),
            ["liquid-glass", "grouped-logbook", "map-sheet"]
        )
        XCTAssertEqual(
            AppearanceMode.allCases.map(\.rawValue),
            ["system", "light", "dark"]
        )
    }

    func testEveryLayoutIsDescribedWellEnoughToChooseFrom() {
        for layout in DashboardLayout.allCases {
            XCTAssertFalse(layout.title.isEmpty, "\(layout.rawValue) needs a title")
            XCTAssertGreaterThan(
                layout.summary.count,
                40,
                "\(layout.rawValue) needs a summary an operator can pick from"
            )
            XCTAssertFalse(layout.systemImage.isEmpty, "\(layout.rawValue) needs an icon")
        }
    }

    func testDefaultsAndFallbacks() {
        let settings = AppSettings()
        XCTAssertEqual(settings.effectiveDashboardLayout, .liquidGlass)
        XCTAssertNil(settings.effectiveColorScheme, "the default follows the system")

        // A logbook created before layout switching existed has no value stored.
        settings.dashboardLayout = nil
        XCTAssertEqual(settings.effectiveDashboardLayout, .liquidGlass)

        // A value from a newer build must not leave the operator on a blank screen.
        settings.dashboardLayout = "holodeck"
        XCTAssertEqual(settings.effectiveDashboardLayout, .liquidGlass)

        settings.dashboardLayout = DashboardLayout.mapSheet.rawValue
        XCTAssertEqual(settings.effectiveDashboardLayout, .mapSheet)
    }

    func testAppearanceModeResolvesToAColorScheme() {
        let settings = AppSettings()
        settings.appearance = "dark"
        XCTAssertEqual(settings.effectiveColorScheme, .dark)
        settings.appearance = "light"
        XCTAssertEqual(settings.effectiveColorScheme, .light)
        settings.appearance = "system"
        XCTAssertNil(settings.effectiveColorScheme)
        settings.appearance = "solarized"
        XCTAssertNil(settings.effectiveColorScheme, "an unknown mode follows the system")
    }

    /// The chosen dashboard has to reach the Rust settings store, or it is lost
    /// on the next sync and the operator's other devices never learn about it.
    func testDashboardLayoutRoundTripsThroughTheRustSettingsPayload() {
        let settings = AppSettings()
        settings.dashboardLayout = DashboardLayout.groupedLogbook.rawValue

        let payload = settings.rustSettingsPayload()
        XCTAssertEqual(payload.display.mobileDashboardLayout, "grouped-logbook")

        let restored = AppSettings()
        restored.apply(rust: payload)
        XCTAssertEqual(restored.effectiveDashboardLayout, .groupedLogbook)
    }

    /// Settings written before the field existed decode with it absent; the app
    /// must keep whatever the device already had rather than resetting.
    func testMissingLayoutInRustPayloadLeavesTheLocalChoiceAlone() {
        let settings = AppSettings()
        settings.dashboardLayout = DashboardLayout.mapSheet.rawValue

        var payload = settings.rustSettingsPayload()
        payload.display.mobileDashboardLayout = nil
        settings.apply(rust: payload)

        XCTAssertEqual(settings.effectiveDashboardLayout, .mapSheet)
    }
}
