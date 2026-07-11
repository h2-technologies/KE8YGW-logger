import SwiftUI

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
