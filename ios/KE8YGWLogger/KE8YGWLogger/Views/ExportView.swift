import SwiftData
import SwiftUI

struct ExportView: View {
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
            rebuildExportFiles()
        }
    }

    private func rebuildExportFiles() {
        do {
            adifURL = try LogExportService.writeTemporaryExportFile(
                name: "KE8YGW-Logger.adi",
                contents: LogExportService.adif(for: qsos)
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
}
