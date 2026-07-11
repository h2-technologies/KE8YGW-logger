import SwiftData
import SwiftUI

@main
struct KE8YGWLoggerApp: App {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
        .modelContainer(for: [QSO.self, StationProfile.self, StationEquipment.self, AppSettings.self])
    }
}
