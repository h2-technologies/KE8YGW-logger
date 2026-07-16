import SwiftData
import SwiftUI

struct QSODetailView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var qsos: [QSO]
    let qso: QSO
    @State private var deleteMessage: String?
    @State private var isDeleting = false

    var body: some View {
        List {
            Section("Contact") {
                detail("Callsign", qso.callsign)
                detail("Date", qso.contactDate.formatted(date: .abbreviated, time: .standard))
                detail("Type", qso.qsoKind.capitalized)
                detail("Band", qso.band)
                detail("Mode", qso.mode)
                detail("Submode", qso.submode)
                detail("Frequency", "\(String(format: "%.6f", qso.frequencyMHz)) MHz")
                detail("RST Sent", qso.rstSent)
                detail("RST Received", qso.rstReceived)
                detail("Power", qso.powerWatts > 0 ? "\(String(format: "%.0f", qso.powerWatts)) W" : "")
            }

            Section("Station") {
                detail("Operator", qso.operatorCallsign)
                detail("Station", qso.stationCallsign)
                detail("Equipment", qso.equipmentSummary)
                detail("Grid", qso.gridSquare)
            }

            Section("Details") {
                detail("Name", qso.name)
                detail("QTH", qso.qth)
                detail("County", qso.county)
                detail("State", qso.state)
                detail("Country", qso.country)
                detail("Contest Exchange", qso.contestExchange)
                detail("Satellite", qso.satelliteName)
                detail("POTA", qso.potaReferences)
                detail("SOTA", qso.sotaReferences)
                detail("Notes", qso.notes)
                detail("Upload", qso.uploadStatus)
                detail("Sync", qso.syncStatus)
            }

            Section {
                Button(isDeleting ? "Deleting" : "Delete QSO", role: .destructive) {
                    Task { await deleteQSO() }
                }
                .disabled(isDeleting)
                if let deleteMessage {
                    Text(deleteMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(qso.callsign)
        .toolbar {
            Button("Edit") {
            }
            .disabled(true)
        }
    }

    private func detail(_ title: String, _ value: String) -> some View {
        HStack(alignment: .top) {
            Text(title)
            Spacer()
            Text(value.isEmpty ? "—" : value)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.trailing)
        }
    }

    private func deleteQSO() async {
        isDeleting = true
        defer { isDeleting = false }
        do {
            guard !qso.canonicalID.isEmpty else {
                qso.isTombstoned = true
                qso.syncStatus = "legacy_cache_removed"
                qso.lastProjectionRefreshAt = Date()
                try modelContext.save()
                dismiss()
                return
            }
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let operationID = UUID().uuidString
            let result = try await bridge.deleteQSO(DeleteQSOBridgeRequest(
                appSupportDir: supportURL.path,
                qsoId: qso.canonicalID,
                operationId: operationID,
                deviceId: nil
            ))
            guard let record = result.qso else {
                throw RustBridgeError.invalidResponse
            }
            _ = try ProjectionRefreshService.upsertQSO(
                from: record,
                event: result.officialEvent,
                operationID: operationID,
                existing: qsos,
                modelContext: modelContext
            )
            dismiss()
        } catch {
            deleteMessage = error.localizedDescription
        }
    }
}
