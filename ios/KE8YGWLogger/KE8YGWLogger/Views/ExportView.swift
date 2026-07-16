import SwiftData
import SwiftUI

struct ExportView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query(sort: \QSO.contactDate, order: .forward) private var qsos: [QSO]
    @State private var adifURL: URL?
    @State private var csvURL: URL?
    @State private var exportError: String?

    var body: some View {
        List {
            Section {
                Text("\(qsos.count) QSOs ready to export.")
                    .foregroundStyle(.secondary)
            }

            Section("ADIF") {
                if let adifURL {
                    ShareLink(item: adifURL) {
                        Label("Share ADIF", systemImage: "square.and.arrow.up")
                    }
                } else {
                    Text("Preparing ADIF export...")
                }
            }

            Section("CSV") {
                if let csvURL {
                    ShareLink(item: csvURL) {
                        Label("Share CSV", systemImage: "tablecells")
                    }
                } else {
                    Text("Preparing CSV export...")
                }
            }

            if let exportError {
                Section {
                    Text(exportError)
                        .foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("Export Logs")
        .task(id: qsos.count) {
            await rebuildExportFiles()
        }
    }

    private func rebuildExportFiles() async {
        do {
            let adif = try await rustBackedADIF()
            adifURL = try LogExportService.writeTemporaryExportFile(
                name: "KE8YGW-Logger.adi",
                contents: adif
            )
            csvURL = try LogExportService.writeTemporaryExportFile(
                name: "KE8YGW-Logger.csv",
                contents: LogExportService.csv(for: qsos)
            )
            exportError = nil
        } catch {
            exportError = error.localizedDescription
        }
    }

    private func rustBackedADIF() async throws -> String {
        let payloads = qsos.map { qso in
            [
                "qso_id": qso.id.uuidString,
                "contacted_callsign": qso.callsign,
                "station_callsign": qso.stationCallsign,
                "operator_callsign": qso.operatorCallsign,
                "started_at": ISO8601DateFormatter().string(from: qso.contactDate),
                "band": qso.band,
                "mode": qso.mode,
                "submode": qso.submode,
                "frequency_hz": Int(qso.frequencyMHz * 1_000_000),
                "rst_sent": qso.rstSent,
                "rst_received": qso.rstReceived,
                "power_watts": qso.powerWatts,
                "grid": qso.gridSquare,
                "county": qso.county,
                "name": qso.name,
                "qth": qso.qth,
                "state": qso.state,
                "country": qso.country,
                "notes": qso.notes,
                "my_sig": qso.potaReferences.isEmpty ? (qso.sotaReferences.isEmpty ? "" : "SOTA") : "POTA",
                "my_sig_info": qso.potaReferences.isEmpty ? qso.sotaReferences : qso.potaReferences
            ] as [String: Any]
        }
        let data = try JSONSerialization.data(withJSONObject: payloads)
        let json = String(decoding: data, as: UTF8.self)
        do {
            let export = try await bridge.exportADIF(payloads: json)
            return export.adif
        } catch {
            return LogExportService.adif(for: qsos)
        }
    }
}
