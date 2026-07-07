import SwiftData
import SwiftUI

struct QSODetailView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    let qso: QSO

    var body: some View {
        List {
            Section("Contact") {
                detail("Callsign", qso.callsign)
                detail("Date", qso.contactDate.formatted(date: .abbreviated, time: .standard))
                detail("Band", qso.band)
                detail("Mode", qso.mode)
                detail("Frequency", "\(String(format: "%.6f", qso.frequencyMHz)) MHz")
                detail("RST Sent", qso.rstSent)
                detail("RST Received", qso.rstReceived)
            }

            Section("Station") {
                detail("Operator", qso.operatorCallsign)
                detail("Station", qso.stationCallsign)
                detail("Grid", qso.gridSquare)
            }

            Section("Details") {
                detail("Name", qso.name)
                detail("QTH", qso.qth)
                detail("State", qso.state)
                detail("Country", qso.country)
                detail("Notes", qso.notes)
            }

            Section {
                Button("Delete QSO", role: .destructive) {
                    modelContext.delete(qso)
                    try? modelContext.save()
                    dismiss()
                }
            }
        }
        .navigationTitle(qso.callsign)
        .toolbar {
            Button("Edit") {
                // TODO: Add full QSO edit screen with append-only change history alignment.
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
}
