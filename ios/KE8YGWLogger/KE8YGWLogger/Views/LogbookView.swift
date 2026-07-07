import SwiftData
import SwiftUI

struct LogbookView: View {
    @Environment(\.modelContext) private var modelContext
    @Query(sort: \QSO.contactDate, order: .reverse) private var qsos: [QSO]
    @State private var searchText = ""

    private var filteredQSOs: [QSO] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        guard !query.isEmpty else { return qsos }
        return qsos.filter { $0.callsign.uppercased().contains(query) }
    }

    var body: some View {
        List {
            if filteredQSOs.isEmpty {
                ContentUnavailableView("No QSOs", systemImage: "book.closed", description: Text("Log a contact from New QSO."))
            } else {
                ForEach(filteredQSOs) { qso in
                    NavigationLink(destination: QSODetailView(qso: qso)) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(qso.callsign)
                                .font(.headline)
                            Text("\(qso.band) \(qso.mode) \(String(format: "%.3f", qso.frequencyMHz)) MHz")
                                .foregroundStyle(.secondary)
                            Text(qso.contactDate, style: .date)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .onDelete(perform: delete)
            }
        }
        .searchable(text: $searchText, prompt: "Search callsign")
        .navigationTitle("Logbook")
        .toolbar {
            NavigationLink("New QSO", destination: NewQSOView())
        }
    }

    private func delete(at offsets: IndexSet) {
        for index in offsets {
            modelContext.delete(filteredQSOs[index])
        }
        try? modelContext.save()
    }
}
