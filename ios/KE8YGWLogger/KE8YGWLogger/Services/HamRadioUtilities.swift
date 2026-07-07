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
