import Foundation
import SwiftData

@Model
final class AppSettings: Identifiable {
    var id: UUID
    var defaultBand: String
    var defaultMode: String
    var autoUppercaseCallsigns: Bool
    var askForLocationLater: Bool
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        defaultBand: String = "20m",
        defaultMode: String = "SSB",
        autoUppercaseCallsigns: Bool = true,
        askForLocationLater: Bool = false,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.defaultBand = defaultBand
        self.defaultMode = defaultMode
        self.autoUppercaseCallsigns = autoUppercaseCallsigns
        self.askForLocationLater = askForLocationLater
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
