import SwiftData
import SwiftUI

struct LogbookView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query(sort: \QSO.contactDate, order: .reverse) private var qsos: [QSO]
    @State private var searchText = ""
    @State private var deletionMessage: String?

    private var filteredQSOs: [QSO] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        let visible = qsos.filter { !$0.isTombstoned }
        guard !query.isEmpty else { return visible }
        return visible.filter { $0.callsign.uppercased().contains(query) }
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
                            Text("\(qso.qsoKind.capitalized) \(qso.band) \(qso.mode) \(String(format: "%.3f", qso.frequencyMHz)) MHz")
                                .foregroundStyle(.secondary)
                            HStack {
                                Text(qso.contactDate, style: .date)
                                Text(qso.syncStatus.capitalized)
                                Text(qso.uploadStatus.capitalized)
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }
                }
                .onDelete(perform: delete)
            }
            if let deletionMessage {
                Section {
                    Text(deletionMessage)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .searchable(text: $searchText, prompt: "Search callsign")
        .navigationTitle("Logbook")
        .toolbar {
            NavigationLink("New QSO", destination: NewQSOView())
        }
    }

    private func delete(at offsets: IndexSet) {
        let targets = offsets.map { filteredQSOs[$0] }
        Task {
            for qso in targets {
                await delete(qso)
            }
        }
    }

    private func delete(_ qso: QSO) async {
        do {
            guard !qso.canonicalID.isEmpty else {
                qso.isTombstoned = true
                qso.syncStatus = "legacy_cache_removed"
                qso.lastProjectionRefreshAt = Date()
                try modelContext.save()
                deletionMessage = "Legacy cache row hidden locally; no Rust canonical ID was available."
                return
            }
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let result = try await bridge.deleteQSO(DeleteQSOBridgeRequest(
                appSupportDir: supportURL.path,
                qsoId: qso.canonicalID,
                operationId: UUID().uuidString,
                deviceId: nil
            ))
            guard let record = result.qso else {
                throw RustBridgeError.invalidResponse
            }
            _ = try ProjectionRefreshService.upsertQSO(
                from: record,
                event: result.officialEvent,
                operationID: record.payload.clientOperationId ?? UUID().uuidString,
                existing: qsos,
                modelContext: modelContext
            )
            deletionMessage = nil
        } catch {
            deletionMessage = error.localizedDescription
        }
    }
}
