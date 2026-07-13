import Foundation

enum HamRadioUtilities {
    static let phoneModes: Set<String> = ["AM", "FM", "SSB", "USB", "LSB"]
    static let cwDigitalModes: Set<String> = ["CW", "FT8", "FT4", "RTTY", "PSK31", "JS8", "DIGI"]

    static func normalizeCallsign(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    }

    static func isValidCallsign(_ value: String) -> Bool {
        let callsign = normalizeCallsign(value)
        guard !callsign.isEmpty, callsign.count >= 3, callsign.count <= 15 else {
            return false
        }
        let pattern = #"^[A-Z0-9]{1,4}[0-9][A-Z0-9/]{1,10}$"#
        return callsign.range(of: pattern, options: .regularExpression) != nil
    }

    static func defaultRST(for mode: String) -> String {
        let normalized = mode.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        if cwDigitalModes.contains(normalized) {
            return "599"
        }
        return "59"
    }

    static func bandFromFrequencyMHz(_ frequencyMHz: Double) -> String? {
        switch frequencyMHz {
        case 1.8..<2.0: return "160m"
        case 3.5..<4.0: return "80m"
        case 5.0..<5.5: return "60m"
        case 7.0..<7.3: return "40m"
        case 10.1..<10.15: return "30m"
        case 14.0..<14.35: return "20m"
        case 18.068..<18.168: return "17m"
        case 21.0..<21.45: return "15m"
        case 24.89..<24.99: return "12m"
        case 28.0..<29.7: return "10m"
        case 50.0..<54.0: return "6m"
        case 144.0..<148.0: return "2m"
        case 222.0..<225.0: return "1.25m"
        case 420.0..<450.0: return "70cm"
        default: return nil
        }
    }

    static func adifEscaped(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\r\n", with: " ")
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func csvEscaped(_ value: String) -> String {
        let escaped = value.replacingOccurrences(of: "\"", with: "\"\"")
        if escaped.contains(",") || escaped.contains("\n") || escaped.contains("\"") {
            return "\"\(escaped)\""
        }
        return escaped
    }

    static func normalizedMaidenhead(_ value: String) -> String? {
        let grid = value.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        guard grid.count == 4 || grid.count == 6 else { return nil }
        let characters = Array(grid)
        let fieldLetters = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQR")
        let squareDigits = CharacterSet.decimalDigits
        let subsquareLetters = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWX")

        guard String(characters[0]).rangeOfCharacter(from: fieldLetters) != nil,
              String(characters[1]).rangeOfCharacter(from: fieldLetters) != nil,
              String(characters[2]).rangeOfCharacter(from: squareDigits) != nil,
              String(characters[3]).rangeOfCharacter(from: squareDigits) != nil else {
            return nil
        }

        if characters.count == 6 {
            guard String(characters[4]).rangeOfCharacter(from: subsquareLetters) != nil,
                  String(characters[5]).rangeOfCharacter(from: subsquareLetters) != nil else {
                return nil
            }
        }

        return grid
    }

    static func maidenheadGrid(latitude: Double, longitude: Double, precision: Int = 6) -> String? {
        guard (-90...90).contains(latitude), (-180...180).contains(longitude) else { return nil }
        let clampedPrecision = precision >= 6 ? 6 : 4
        let lon = min(max(longitude + 180.0, 0.0), 359.999999)
        let lat = min(max(latitude + 90.0, 0.0), 179.999999)

        let fieldLon = Int(lon / 20.0)
        let fieldLat = Int(lat / 10.0)
        let squareLon = Int((lon.truncatingRemainder(dividingBy: 20.0)) / 2.0)
        let squareLat = Int(lat.truncatingRemainder(dividingBy: 10.0))

        var grid = ""
        grid.append(Character(UnicodeScalar(65 + fieldLon)!))
        grid.append(Character(UnicodeScalar(65 + fieldLat)!))
        grid += "\(squareLon)\(squareLat)"

        if clampedPrecision == 6 {
            let lonRemainder = lon - (Double(fieldLon) * 20.0) - (Double(squareLon) * 2.0)
            let latRemainder = lat - (Double(fieldLat) * 10.0) - Double(squareLat)
            let subsquareLon = Int(lonRemainder / (2.0 / 24.0))
            let subsquareLat = Int(latRemainder / (1.0 / 24.0))
            grid.append(Character(UnicodeScalar(65 + min(max(subsquareLon, 0), 23))!))
            grid.append(Character(UnicodeScalar(65 + min(max(subsquareLat, 0), 23))!))
        }

        return grid
    }
}

enum MaidenheadLocationSource: String, CaseIterable, Identifiable, Codable {
    case manual
    case gps
    case cachedGPS
    case stationDefault
    case unknown

    var id: String { rawValue }

    var label: String {
        switch self {
        case .manual: return "Manual"
        case .gps: return "GPS"
        case .cachedGPS: return "Cached GPS"
        case .stationDefault: return "Station Default"
        case .unknown: return "Unknown"
        }
    }
}

enum NetTrafficClassification: String, CaseIterable, Identifiable, Codable {
    case emergency
    case priority
    case routine
    case healthAndWelfare = "health_and_welfare"
    case noTraffic = "no_traffic"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .emergency: return "Emergency"
        case .priority: return "Priority"
        case .routine: return "Routine"
        case .healthAndWelfare: return "Health and Welfare"
        case .noTraffic: return "No Traffic"
        }
    }

    var sortRank: Int {
        switch self {
        case .emergency: return 0
        case .priority: return 1
        case .routine: return 2
        case .healthAndWelfare: return 3
        case .noTraffic: return 4
        }
    }

    var symbolName: String {
        switch self {
        case .emergency: return "exclamationmark.triangle.fill"
        case .priority: return "flag.fill"
        case .routine: return "checkmark.circle"
        case .healthAndWelfare: return "heart.text.square"
        case .noTraffic: return "minus.circle"
        }
    }

    static let trafficCases: [NetTrafficClassification] = [.emergency, .priority, .routine, .healthAndWelfare]
}

struct NetCheckIn: Identifiable, Codable, Equatable {
    var id: UUID = UUID()
    var callsign: String
    var name: String
    var location: String
    var late: Bool
    var classification: NetTrafficClassification
    var checkedInAt: Date = Date()

    var emergencyTraffic: Bool { classification == .emergency }
}

struct NetTrafficItem: Identifiable, Codable, Equatable {
    var id: UUID = UUID()
    var from: String
    var to: String
    var summary: String
    var classification: NetTrafficClassification
    var createdAt: Date = Date()

    var emergency: Bool { classification == .emergency }
}

struct ActivationDraft: Codable, Equatable {
    var reference: String = ""
    var startedAt: Date?
    var activationID: String?
    var spotFrequency: String = ""
    var mode: String = "SSB"
    var message: String?
    var offlineOnly: Bool = false
}

struct QSOFormDraft: Codable, Equatable {
    var callsign = ""
    var contactDate = Date()
    var qsoKind = "Voice"
    var band = "20m"
    var mode = "SSB"
    var submode = ""
    var frequencyMHz = ""
    var rstSent = "59"
    var rstReceived = "59"
    var powerWatts = ""
    var gridSquare = ""
    var county = ""
    var name = ""
    var qth = ""
    var state = ""
    var country = ""
    var contestExchange = ""
    var satelliteName = ""
    var potaReferences = ""
    var sotaReferences = ""
    var notes = ""
}

struct NetControlDraft: Codable, Equatable {
    var netName = "Weekly Emergency Net"
    var frequency = "146.520"
    var activeSince: Date?
    var netSessionID: String?
    var callsign = ""
    var operatorName = ""
    var location = ""
    var lateCheckIn = false
    var checkInClassification = NetTrafficClassification.noTraffic
    var checkIns: [NetCheckIn] = []
    var traffic: [NetTrafficItem] = []
    var assignment = ""
    var assignments: [String] = []
    var trafficFrom = ""
    var trafficTo = "NCS"
    var trafficSummary = ""
    var trafficClassification = NetTrafficClassification.emergency
    var netMessage: String?
}

enum ProviderCredentialFieldKind: String, Codable {
    case text
    case secure
    case number
    case url
}

struct ProviderCredentialField: Identifiable, Codable, Hashable {
    var id: String
    var label: String
    var kind: ProviderCredentialFieldKind
    var required: Bool
}

struct ProviderCredentialDefinition: Identifiable, Codable, Hashable {
    var id: String
    var displayName: String
    var statusKey: String
    var aliases: [String] = []
    var fields: [ProviderCredentialField]

    var secureFields: [ProviderCredentialField] {
        fields.filter { $0.kind == .secure }
    }

    var nonSecretFields: [ProviderCredentialField] {
        fields.filter { $0.kind != .secure }
    }
}

enum ProviderCredentialCatalog {
    static let definitions: [ProviderCredentialDefinition] = [
        ProviderCredentialDefinition(
            id: "qrz",
            displayName: "QRZ XML",
            statusKey: "qrz",
            aliases: ["qrz-xml"],
            fields: [
                ProviderCredentialField(id: "username", label: "Username", kind: .text, required: true),
                ProviderCredentialField(id: "password", label: "Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "qrz-logbook",
            displayName: "QRZ Logbook",
            statusKey: "qrz_logbook",
            aliases: ["qrz_logbook"],
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "api_key", label: "API Key", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "hrdlog",
            displayName: "HRDLog",
            statusKey: "hrdlog",
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "upload_code", label: "Upload Code", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "hamqth",
            displayName: "HamQTH",
            statusKey: "hamqth",
            fields: [
                ProviderCredentialField(id: "username", label: "Username", kind: .text, required: true),
                ProviderCredentialField(id: "password", label: "Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "pota",
            displayName: "POTA",
            statusKey: "pota",
            aliases: ["pota-spots"],
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "api_key", label: "API Key", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "sotawatch",
            displayName: "SOTAWatch",
            statusKey: "sotawatch",
            aliases: ["sota", "sota-watch"],
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "api_key", label: "API Key or Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "club-log",
            displayName: "Club Log",
            statusKey: "club_log",
            aliases: ["clublog", "club_log"],
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "email", label: "Email", kind: .text, required: true),
                ProviderCredentialField(id: "password", label: "Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "eqsl",
            displayName: "eQSL",
            statusKey: "eqsl",
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "password", label: "Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "lotw",
            displayName: "LoTW",
            statusKey: "lotw",
            fields: [
                ProviderCredentialField(id: "callsign", label: "Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "certificate_password", label: "Certificate Password", kind: .secure, required: true)
            ]
        ),
        ProviderCredentialDefinition(
            id: "dx-cluster",
            displayName: "DX Cluster",
            statusKey: "dx_cluster",
            aliases: ["dx_cluster"],
            fields: [
                ProviderCredentialField(id: "callsign", label: "Login Callsign", kind: .text, required: true),
                ProviderCredentialField(id: "host", label: "Host", kind: .text, required: true),
                ProviderCredentialField(id: "port", label: "Port", kind: .number, required: true)
            ]
        )
    ]

    static func definition(for providerID: String) -> ProviderCredentialDefinition? {
        definitions.first { $0.id == providerID || $0.statusKey == providerID || $0.aliases.contains(providerID) }
    }
}

struct ProviderValidationRecord: Codable, Equatable {
    var configured: Bool
    var validated: Bool
    var validatedAt: Date?
    var message: String
}

enum ActivationEligibilityState: Equatable {
    case providerValidated
    case offlineLocalOnly
    case providerDisabled
    case credentialsMissing
    case validationMissingOrStale
}

struct ActivationEligibility: Equatable {
    var state: ActivationEligibilityState
    var canStart: Bool
    var message: String
    var offlineOnly: Bool

    static func evaluate(
        providerID: String,
        settings: AppSettings?,
        networkAvailable: Bool,
        validationTTLHours: Int
    ) -> ActivationEligibility {
        guard let settings else {
            return ActivationEligibility(state: .credentialsMissing, canStart: false, message: "Settings are not available.", offlineOnly: false)
        }
        guard settings.isProviderEnabled(providerID) else {
            return ActivationEligibility(state: .providerDisabled, canStart: false, message: "The provider is disabled.", offlineOnly: false)
        }
        if !networkAvailable {
            return ActivationEligibility(state: .offlineLocalOnly, canStart: settings.allowOfflineActivations, message: "No usable internet connection. Activation will be local-only.", offlineOnly: true)
        }
        let validation = settings.providerValidationRecord(providerID)
        guard validation.configured else {
            return ActivationEligibility(state: .credentialsMissing, canStart: false, message: "Credentials are missing.", offlineOnly: false)
        }
        guard validation.validated, let validatedAt = validation.validatedAt else {
            return ActivationEligibility(state: .validationMissingOrStale, canStart: false, message: "Credentials have not been validated.", offlineOnly: false)
        }
        let maxAge = TimeInterval(max(validationTTLHours, 1) * 3600)
        guard Date().timeIntervalSince(validatedAt) <= maxAge else {
            return ActivationEligibility(state: .validationMissingOrStale, canStart: false, message: "Credential validation is stale. Revalidate in Settings.", offlineOnly: false)
        }
        return ActivationEligibility(state: .providerValidated, canStart: true, message: "Provider credentials are validated.", offlineOnly: false)
    }
}

enum HamDateFormatters {
    static let adifDate: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyyMMdd"
        return formatter
    }()

    static let adifTime: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "HHmmss"
        return formatter
    }()

    static let csvDateTime: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()
}
