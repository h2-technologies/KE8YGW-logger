import SwiftData
import SwiftUI

struct POTAView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var qsos: [QSO]
    @State private var parkReference = ""
    @State private var activationStartedAt: Date?
    @State private var activationID: String?
    @State private var spotFrequency = "14.244"
    @State private var mode = "SSB"
    @State private var spotMessage: String?
    @State private var activationMessage: String?

    private var activationQSOs: [QSO] {
        qsos.filter { !$0.potaReferences.isEmpty || $0.qsoKind == "pota" }
    }

    var body: some View {
        List {
            Section("Activation") {
                TextField("Park Reference", text: $parkReference)
                    .textInputAutocapitalization(.characters)
                TextField("Spot Frequency MHz", text: $spotFrequency)
                    .keyboardType(.decimalPad)
                TextField("Mode", text: $mode)
                    .textInputAutocapitalization(.characters)
                if let activationStartedAt {
                    DetailRow(title: "Started", value: activationStartedAt.formatted(date: .omitted, time: .standard))
                    DetailRow(title: "Elapsed", value: elapsedText(since: activationStartedAt))
                }
                Button(activationStartedAt == nil ? "Start Activation" : "End Activation") {
                    Task { await toggleActivation() }
                }
                .disabled(parkReference.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if let activationMessage {
                    Text(activationMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section("Statistics") {
                DetailRow(title: "QSOs", value: "\(activationQSOs.count)")
                DetailRow(title: "Unique Calls", value: "\(Set(activationQSOs.map { $0.callsign }).count)")
                DetailRow(title: "Bands", value: Set(activationQSOs.map { $0.band }).sorted().joined(separator: ", "))
            }

            Section("Spotting") {
                Button("Post Spot") {
                    spotMessage = "Spot queued for \(parkReference) on \(spotFrequency) MHz \(mode)."
                }
                    .disabled(parkReference.isEmpty)
                if let spotMessage {
                    Text(spotMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Label("POTA provider status is supplied by the Rust provider bridge.", systemImage: "point.3.connected.trianglepath.dotted")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Export") {
                NavigationLink("Export Logs", destination: ExportView())
            }
        }
        .navigationTitle("POTA")
    }

    private func elapsedText(since start: Date) -> String {
        let seconds = Int(Date().timeIntervalSince(start))
        return "\(seconds / 3600)h \((seconds / 60) % 60)m"
    }

    private func toggleActivation() async {
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            if let activationID {
                _ = try await bridge.endActivation(ActivationEndBridgeRequest(
                    appSupportDir: supportURL.path,
                    activationId: activationID,
                    endedAt: Self.isoFormatter.string(from: Date())
                ))
                self.activationID = nil
                activationStartedAt = nil
                activationMessage = "Activation ended through Rust."
            } else {
                let startedAt = Date()
                let result = try await bridge.startActivation(ActivationBridgeRequest(
                    appSupportDir: supportURL.path,
                    activationType: "pota",
                    stationCallsign: bridge.dashboard.activeStation?.stationCallsign ?? "KE8YGW",
                    operatorCallsign: bridge.dashboard.operatorCallsign,
                    startedAt: Self.isoFormatter.string(from: startedAt),
                    parkId: parkReference.uppercased(),
                    summitId: nil,
                    grid: bridge.dashboard.gps?.grid,
                    locationName: nil,
                    notes: nil
                ))
                activationID = result.officialEvent.entityId
                activationStartedAt = startedAt
                activationMessage = "Activation started through Rust."
            }
        } catch {
            activationMessage = error.localizedDescription
        }
    }

    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
