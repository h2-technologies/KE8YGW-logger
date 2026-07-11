import SwiftUI

struct NetCheckIn: Identifiable {
    var id = UUID()
    var callsign: String
    var name: String
    var location: String
    var late: Bool
    var emergencyTraffic: Bool
}

struct NetTrafficItem: Identifiable {
    var id = UUID()
    var from: String
    var to: String
    var summary: String
    var emergency: Bool
}

struct NetControlView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @State private var netName = "Weekly Emergency Net"
    @State private var frequency = "146.520"
    @State private var activeSince: Date?
    @State private var netSessionID: String?
    @State private var callsign = ""
    @State private var operatorName = ""
    @State private var location = ""
    @State private var lateCheckIn = false
    @State private var emergencyTraffic = false
    @State private var checkIns: [NetCheckIn] = []
    @State private var traffic: [NetTrafficItem] = []
    @State private var assignment = ""
    @State private var assignments: [String] = []
    @State private var netMessage: String?

    var body: some View {
        List {
            Section("Session") {
                TextField("Net Name", text: $netName)
                TextField("Frequency MHz", text: $frequency)
                    .keyboardType(.decimalPad)
                if let activeSince {
                    DetailRow(title: "Active", value: activeSince.formatted(date: .omitted, time: .standard))
                }
                Button(activeSince == nil ? "Start Net" : "End Net") {
                    Task { await toggleNetSession() }
                }
                if let netMessage {
                    Text(netMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Section("Check-In") {
                TextField("Callsign", text: $callsign)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                TextField("Name", text: $operatorName)
                TextField("Location", text: $location)
                Toggle("Late Check-In", isOn: $lateCheckIn)
                Toggle("Emergency Traffic", isOn: $emergencyTraffic)
                Button("Add Check-In") {
                    Task { await addCheckIn() }
                }
                    .disabled(callsign.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            Section("Roster") {
                if checkIns.isEmpty {
                    Text("No check-ins yet.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(checkIns) { item in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(item.callsign)
                                    .font(.headline)
                                Spacer()
                                if item.emergencyTraffic {
                                    Text("Emergency")
                                        .foregroundStyle(.red)
                                } else if item.late {
                                    Text("Late")
                                        .foregroundStyle(.orange)
                                }
                            }
                            Text([item.name, item.location].filter { !$0.isEmpty }.joined(separator: " - "))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            Section("Traffic & Assignments") {
                TextField("Assignment", text: $assignment)
                Button("Create Assignment") {
                    assignments.append(assignment)
                    assignment = ""
                }
                    .disabled(assignment.isEmpty)
                Button("Add Emergency Traffic") {
                    Task { await addEmergencyTraffic() }
                }
                DetailRow(title: "Traffic Items", value: "\(traffic.count)")
                DetailRow(title: "Emergency Items", value: "\(traffic.filter { $0.emergency }.count)")
                ForEach(assignments, id: \.self) { item in
                    Text(item)
                        .font(.caption)
                }
            }

            Section("Export") {
                ShareLink(item: reportText, subject: Text("Net Report")) {
                    Label("Share Net Report", systemImage: "square.and.arrow.up")
                }
            }
        }
        .navigationTitle("Net Control")
    }

    private var reportText: String {
        var output = "# \(netName)\n\nFrequency: \(frequency) MHz\nCheck-ins: \(checkIns.count)\n\n"
        for item in checkIns {
            output += "- \(item.callsign) \(item.name) \(item.location)\n"
        }
        return output
    }

    private func toggleNetSession() async {
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            if let netSessionID {
                _ = try await bridge.endNetSession(NetSessionEndBridgeRequest(
                    appSupportDir: supportURL.path,
                    netSessionId: netSessionID,
                    endedAt: Self.isoFormatter.string(from: Date())
                ))
                self.netSessionID = nil
                activeSince = nil
                netMessage = "Net closed through Rust."
            } else {
                let startedAt = Date()
                let result = try await bridge.startNetSession(NetSessionStartBridgeRequest(
                    appSupportDir: supportURL.path,
                    netName: netName,
                    stationCallsign: bridge.dashboard.activeStation?.stationCallsign ?? "KE8YGW",
                    netControlOperatorId: bridge.dashboard.operatorCallsign,
                    startedAt: Self.isoFormatter.string(from: startedAt),
                    frequencyHz: frequencyHz,
                    band: nil,
                    mode: "FM",
                    notes: nil
                ))
                netSessionID = result.officialEvent.entityId
                activeSince = startedAt
                netMessage = "Net started through Rust."
            }
        } catch {
            netMessage = error.localizedDescription
        }
    }

    private func addCheckIn() async {
        do {
            guard let netSessionID else {
                netMessage = "Start a net before adding check-ins."
                return
            }
            let normalized = HamRadioUtilities.normalizeCallsign(callsign)
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            _ = try await bridge.createNetCheckIn(NetCheckInBridgeRequest(
                appSupportDir: supportURL.path,
                netSessionId: netSessionID,
                callsign: normalized,
                operatorName: operatorName,
                location: location,
                grid: nil,
                tacticalCallsign: nil,
                status: emergencyTraffic ? "emergency" : "checked_in",
                traffic: emergencyTraffic ? "Emergency traffic" : nil,
                checkinTime: Self.isoFormatter.string(from: Date()),
                late: lateCheckIn,
                emergencyTraffic: emergencyTraffic
            ))
            checkIns.append(NetCheckIn(
                callsign: normalized,
                name: operatorName,
                location: location,
                late: lateCheckIn,
                emergencyTraffic: emergencyTraffic
            ))
            callsign = ""
            operatorName = ""
            location = ""
            lateCheckIn = false
            emergencyTraffic = false
            netMessage = nil
        } catch {
            netMessage = error.localizedDescription
        }
    }

    private func addEmergencyTraffic() async {
        do {
            guard let netSessionID else {
                netMessage = "Start a net before adding traffic."
                return
            }
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            _ = try await bridge.createNetTraffic(NetTrafficBridgeRequest(
                appSupportDir: supportURL.path,
                netSessionId: netSessionID,
                fromCallsign: callsign.isEmpty ? nil : HamRadioUtilities.normalizeCallsign(callsign),
                toCallsign: "NCS",
                precedence: "emergency",
                summary: "Emergency traffic"
            ))
            traffic.append(NetTrafficItem(from: callsign, to: "NCS", summary: "Emergency traffic", emergency: true))
            netMessage = nil
        } catch {
            netMessage = error.localizedDescription
        }
    }

    private var frequencyHz: UInt64? {
        guard let mhz = Double(frequency.trimmingCharacters(in: .whitespacesAndNewlines)), mhz > 0 else {
            return nil
        }
        return UInt64((mhz * 1_000_000).rounded())
    }

    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}

struct EmergencyCommsView: View {
    @State private var incidentName = ""
    @State private var tacticalCall = ""
    @State private var operationalPeriod = ""
    @State private var assignment = ""
    @State private var assignments: [String] = []

    var body: some View {
        List {
            Section("Incident") {
                TextField("Incident Name", text: $incidentName)
                TextField("Tactical Call", text: $tacticalCall)
                    .textInputAutocapitalization(.characters)
                TextField("Operational Period", text: $operationalPeriod)
            }

            Section("Assignments") {
                TextField("Assignment", text: $assignment, axis: .vertical)
                    .lineLimit(2...5)
                Button("Add Assignment") {
                    assignments.append(assignment)
                    assignment = ""
                }
                    .disabled(assignment.isEmpty)
                ForEach(assignments, id: \.self) { item in
                    Text(item)
                }
            }

            Section("Operational Tools") {
                NavigationLink("Open Net Control", destination: NetControlView())
                NavigationLink("Open Map", destination: MapWorkspaceView())
                NavigationLink("Diagnostics", destination: DiagnosticsView())
            }
        }
        .navigationTitle("Emergency")
    }
}
