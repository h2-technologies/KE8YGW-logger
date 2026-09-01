import SwiftUI

/// The three phone dashboards an operator can switch between. Each one shows the
/// same data; they differ in what is reachable without scrolling and in how much
/// of the screen the map gets.
///
/// The raw values are the identifiers persisted in the shared Rust settings
/// (`display.mobile_dashboard_layout`), so renaming one is a settings migration.
enum DashboardLayout: String, CaseIterable, Identifiable, Hashable {
    case liquidGlass = "liquid-glass"
    case groupedLogbook = "grouped-logbook"
    case mapSheet = "map-sheet"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .liquidGlass: return "Today"
        case .groupedLogbook: return "Logbook"
        case .mapSheet: return "Map & Sheet"
        }
    }

    var summary: String {
        switch self {
        case .liquidGlass:
            return "Live activation card, three counters, then recent contacts and spots worth working. Best for a session you are in the middle of."
        case .groupedLogbook:
            return "A grouped list of contacts by day with swipe actions. Best for reviewing and tidying a log rather than running one."
        case .mapSheet:
            return "Map above, log entry in a sheet that never dismisses. Best for portable operating and park-to-park hunting."
        }
    }

    var systemImage: String {
        switch self {
        case .liquidGlass: return "square.stack.3d.up"
        case .groupedLogbook: return "list.bullet.rectangle"
        case .mapSheet: return "map"
        }
    }
}

/// How the app resolves light and dark. Mirrors `ham_core::APPEARANCE_MODES`.
enum AppearanceMode: String, CaseIterable, Identifiable, Hashable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: return "Match system"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}

enum AppTheme {
    static let compactGrid = [GridItem(.adaptive(minimum: 180), spacing: 12)]

    static func statusColor(_ status: String?) -> Color {
        let value = status?.lowercased() ?? ""
        if value.contains("healthy") || value.contains("connected") || value.contains("ready") {
            return .green
        }
        if value.contains("pending") || value.contains("credential") || value.contains("offline") {
            return .orange
        }
        if value.contains("failed") || value.contains("error") || value.contains("missing") {
            return .red
        }
        return .secondary
    }
}

struct MetricTile: View {
    var title: String
    var value: String
    var systemImage: String
    var tint: Color = .accentColor

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            Text(value)
                .font(.title3.weight(.semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.75)
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
    }
}

struct DetailRow: View {
    var title: String
    var value: String

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
            Spacer(minLength: 12)
            Text(value.isEmpty ? "-" : value)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.trailing)
        }
    }
}
