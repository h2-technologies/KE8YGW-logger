import SwiftData
import SwiftUI

struct LoggingWorkspaceView: View {
    @Query(sort: \QSO.contactDate, order: .reverse) private var qsos: [QSO]

    private var pendingUploads: Int {
        qsos.filter { $0.uploadStatus != "uploaded" }.count
    }

    var body: some View {
        List {
            Section {
                NavigationLink(destination: NewQSOView()) {
                    Label("New QSO", systemImage: "plus.circle")
                }
                NavigationLink(destination: LogbookView()) {
                    Label("Open Logbook", systemImage: "book")
                }
            }

            Section("Modes") {
                modeRow("Voice", "mic")
                modeRow("CW", "waveform")
                modeRow("Digital", "keyboard")
                modeRow("Satellite", "dot.radiowaves.left.and.right")
                modeRow("Contest", "flag.checkered")
                modeRow("Net", "person.3.sequence")
                modeRow("Emergency", "cross.case")
                modeRow("POTA", "tree")
                modeRow("SOTA", "mountain.2")
            }

            Section("Offline Queue") {
                DetailRow(title: "Local QSOs", value: "\(qsos.count)")
                DetailRow(title: "Pending Upload", value: "\(pendingUploads)")
                DetailRow(title: "Storage", value: "SwiftData cache; Rust event bridge pending link")
            }
        }
        .navigationTitle("Logging")
    }

    private func modeRow(_ title: String, _ image: String) -> some View {
        Label(title, systemImage: image)
            .foregroundStyle(.primary)
    }
}
