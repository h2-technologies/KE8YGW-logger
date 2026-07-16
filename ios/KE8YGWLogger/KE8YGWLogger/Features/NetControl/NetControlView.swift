import SwiftData
import SwiftUI

struct NetControlView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var settings: [AppSettings]

    @State private var netName = "Weekly Emergency Net"
    @State private var frequency = "146.520"
    @State private var activeSince: Date?
    @State private var netSessionID: String?
    @State private var callsign = ""
    @State private var operatorName = ""
    @State private var location = ""
    @State private var lateCheckIn = false
    @State private var checkInClassification = NetTrafficClassification.noTraffic
    @State private var checkIns: [NetCheckIn] = []
    @State private var traffic: [NetTrafficItem] = []
    @State private var assignment = ""
    @State private var assignments: [String] = []
    @State private var trafficFrom = ""
    @State private var trafficTo = "NCS"
    @State private var trafficSummary = ""
    @State private var trafficClassification = NetTrafficClassification.emergency
    @State private var netMessage: String?
    @State private var confirmEndNet = false

    private var appSettings: AppSettings? { settings.first }
    private var sortedCheckIns: [NetCheckIn] {
        checkIns.sorted {
            if $0.classification.sortRank != $1.classification.sortRank {
                return $0.classification.sortRank < $1.classification.sortRank
            }
            if $0.checkedInAt != $1.checkedInAt {
                return $0.checkedInAt < $1.checkedInAt
            }
            return $0.callsign < $1.callsign
        }
    }
    private var sortedTraffic: [NetTrafficItem] {
        traffic.sorted {
            if $0.classification.sortRank != $1.classification.sortRank {
                return $0.classification.sortRank < $1.classification.sortRank
            }
            if $0.createdAt != $1.createdAt {
                return $0.createdAt < $1.createdAt
            }
            return $0.from < $1.from
        }
    }

    var body: some View {
        List {
            Section("Session") {
                TextField("Net Name", text: $netName)
                    .onChange(of: netName) { _, _ in persistDraft() }
                TextField("Frequency MHz", text: $frequency)
                    .keyboardType(.decimalPad)
                    .onChange(of: frequency) { _, _ in persistDraft() }
                if let activeSince {
                    DetailRow(title: "Active", value: activeSince.formatted(date: .omitted, time: .standard))
                }
                Button(activeSince == nil ? "Start Net" : "End Net") {
                    if activeSince == nil {
                        Task { await toggleNetSession() }
                    } else {
                        confirmEndNet = true
                    }
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
                    .onChange(of: callsign) { _, _ in persistDraft() }
                TextField("Name", text: $operatorName)
                    .onChange(of: operatorName) { _, _ in persistDraft() }
                TextField("Location", text: $location)
                    .onChange(of: location) { _, _ in persistDraft() }
                Toggle("Late Check-In", isOn: $lateCheckIn)
                    .onChange(of: lateCheckIn) { _, _ in persistDraft() }
                Picker("Classification", selection: $checkInClassification) {
                    ForEach(NetTrafficClassification.allCases) { classification in
                        Label(classification.label, systemImage: classification.symbolName).tag(classification)
                    }
                }
                .onChange(of: checkInClassification) { _, _ in persistDraft() }
                Button("Add Check-In") {
                    Task { await addCheckIn() }
                }
                .disabled(callsign.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            Section("Roster") {
                if sortedCheckIns.isEmpty {
                    Text("No check-ins yet.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(sortedCheckIns) { item in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(item.callsign)
                                    .font(.headline)
                                Spacer()
                                Picker("Classification", selection: classificationBinding(for: item.id)) {
                                    ForEach(NetTrafficClassification.allCases) { classification in
                                        Text(classification.label).tag(classification)
                                    }
                                }
                                .labelsHidden()
                            }
                            Label(item.classification.label, systemImage: item.classification.symbolName)
                                .font(.caption)
                                .accessibilityLabel("\(item.callsign) classification \(item.classification.label)")
                            Text([item.name, item.location].filter { !$0.isEmpty }.joined(separator: " - "))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            if item.late {
                                Text("Late check-in")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }

            Section("Traffic") {
                Picker("Precedence", selection: $trafficClassification) {
                    ForEach(NetTrafficClassification.trafficCases) { classification in
                        Label(classification.label, systemImage: classification.symbolName).tag(classification)
                    }
                }
                .onChange(of: trafficClassification) { _, _ in persistDraft() }
                TextField("From", text: $trafficFrom)
                    .textInputAutocapitalization(.characters)
                    .onChange(of: trafficFrom) { _, _ in persistDraft() }
                TextField("To", text: $trafficTo)
                    .textInputAutocapitalization(.characters)
                    .onChange(of: trafficTo) { _, _ in persistDraft() }
                TextField("Summary", text: $trafficSummary, axis: .vertical)
                    .lineLimit(2...5)
                    .onChange(of: trafficSummary) { _, _ in persistDraft() }
                Button("Add Traffic") {
                    Task { await addTraffic() }
                }
                .disabled(trafficSummary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                DetailRow(title: "Traffic Items", value: "\(traffic.count)")
                DetailRow(title: "Emergency Items", value: "\(traffic.filter { $0.classification == .emergency }.count)")
                ForEach(sortedTraffic) { item in
                    VStack(alignment: .leading, spacing: 4) {
                        Label(item.classification.label, systemImage: item.classification.symbolName)
                            .font(.caption)
                            .accessibilityLabel("Traffic classification \(item.classification.label)")
                        Text(item.summary)
                            .font(.headline)
                        Text("From \(item.from.isEmpty ? "Unknown" : item.from) to \(item.to.isEmpty ? "NCS" : item.to)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            Section("Assignments") {
                TextField("Assignment", text: $assignment)
                    .onChange(of: assignment) { _, _ in persistDraft() }
                Button("Create Assignment") {
                    assignments.append(assignment)
                    assignment = ""
                    persistDraft()
                }
                .disabled(assignment.isEmpty)
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
        .task {
            restoreDraft()
        }
        .confirmationDialog("End this Net Control session?", isPresented: $confirmEndNet, titleVisibility: .visible) {
            Button("End Net", role: .destructive) {
                Task { await toggleNetSession() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The net will be ended through the Rust event path and the local active-session draft will be cleared.")
        }
    }

    private var reportText: String {
        var output = "# \(netName)\n\nFrequency: \(frequency) MHz\nCheck-ins: \(checkIns.count)\n\n"
        for item in sortedCheckIns {
            output += "- \(item.callsign) \(item.classification.label) \(item.name) \(item.location)\n"
        }
        for item in sortedTraffic {
            output += "- Traffic: \(item.classification.label) \(item.from) to \(item.to): \(item.summary)\n"
        }
        return output
    }

    private func classificationBinding(for id: UUID) -> Binding<NetTrafficClassification> {
        Binding {
            checkIns.first(where: { $0.id == id })?.classification ?? .noTraffic
        } set: { value in
            if let index = checkIns.firstIndex(where: { $0.id == id }) {
                checkIns[index].classification = value
                persistDraft()
            }
        }
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
                appSettings?.netDraftJSON = ""
                appSettings?.updatedAt = Date()
                try? modelContext.save()
                return
            } else {
                let startedAt = Date()
                let result = try await bridge.startNetSession(NetSessionStartBridgeRequest(
                    appSupportDir: supportURL.path,
                    netName: netName,
                    stationCallsign: bridge.dashboard.activeStation?.stationCallsign ?? appSettings?.stationCallsign ?? "KE8YGW",
                    netControlOperatorId: bridge.dashboard.operatorCallsign,
                    startedAt: Self.isoFormatter.string(from: startedAt),
                    frequencyHz: frequencyHz,
                    band: nil,
                    mode: appSettings?.netDefaultMode ?? "FM",
                    notes: nil
                ))
                netSessionID = result.officialEvent.entityId
                activeSince = startedAt
                netMessage = "Net started through Rust."
            }
            persistDraft()
        } catch {
            netMessage = error.localizedDescription
            persistDraft()
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
                status: lateCheckIn ? "late" : "checked_in",
                traffic: checkInTrafficValue(checkInClassification),
                checkinTime: Self.isoFormatter.string(from: Date()),
                late: lateCheckIn,
                emergencyTraffic: checkInClassification == .emergency
            ))
            checkIns.append(NetCheckIn(
                callsign: normalized,
                name: operatorName,
                location: location,
                late: lateCheckIn,
                classification: checkInClassification,
                checkedInAt: Date()
            ))
            callsign = ""
            operatorName = ""
            location = ""
            lateCheckIn = false
            checkInClassification = .noTraffic
            netMessage = nil
            persistDraft()
        } catch {
            netMessage = error.localizedDescription
            persistDraft()
        }
    }

    private func addTraffic() async {
        do {
            guard let netSessionID else {
                netMessage = "Start a net before adding traffic."
                return
            }
            guard trafficClassification != .noTraffic else {
                netMessage = "No Traffic is a roster state, not a traffic message precedence."
                return
            }
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let summary = trafficSummary.trimmingCharacters(in: .whitespacesAndNewlines)
            _ = try await bridge.createNetTraffic(NetTrafficBridgeRequest(
                appSupportDir: supportURL.path,
                netSessionId: netSessionID,
                fromCallsign: trafficFrom.isEmpty ? nil : HamRadioUtilities.normalizeCallsign(trafficFrom),
                toCallsign: trafficTo.isEmpty ? "NCS" : HamRadioUtilities.normalizeCallsign(trafficTo),
                precedence: trafficPrecedenceValue(trafficClassification),
                summary: trafficClassification == .healthAndWelfare ? "Health and Welfare: \(summary)" : summary
            ))
            traffic.append(NetTrafficItem(
                from: trafficFrom.isEmpty ? "" : HamRadioUtilities.normalizeCallsign(trafficFrom),
                to: trafficTo.isEmpty ? "NCS" : HamRadioUtilities.normalizeCallsign(trafficTo),
                summary: summary,
                classification: trafficClassification,
                createdAt: Date()
            ))
            trafficSummary = ""
            trafficFrom = ""
            trafficTo = "NCS"
            trafficClassification = .emergency
            netMessage = nil
            persistDraft()
        } catch {
            netMessage = error.localizedDescription
            persistDraft()
        }
    }

    private func checkInTrafficValue(_ classification: NetTrafficClassification) -> String? {
        switch classification {
        case .emergency: return "emergency"
        case .priority: return "priority"
        case .routine, .healthAndWelfare: return "listed"
        case .noTraffic: return nil
        }
    }

    private func trafficPrecedenceValue(_ classification: NetTrafficClassification) -> String {
        switch classification {
        case .emergency: return "emergency"
        case .priority: return "priority"
        case .routine, .healthAndWelfare, .noTraffic: return "routine"
        }
    }

    private func restoreDraft() {
        if let settings = appSettings {
            netName = settings.netDefaultName?.isEmpty == false ? settings.netDefaultName ?? netName : netName
            frequency = settings.netDefaultFrequencyMHz?.isEmpty == false ? settings.netDefaultFrequencyMHz ?? frequency : frequency
        }
        guard let data = appSettings?.netDraftJSON?.data(using: .utf8),
              let draft = try? JSONDecoder().decode(NetControlDraft.self, from: data) else { return }
        netName = draft.netName
        frequency = draft.frequency
        activeSince = draft.activeSince
        netSessionID = draft.netSessionID
        callsign = draft.callsign
        operatorName = draft.operatorName
        location = draft.location
        lateCheckIn = draft.lateCheckIn
        checkInClassification = draft.checkInClassification
        checkIns = draft.checkIns
        traffic = draft.traffic
        assignment = draft.assignment
        assignments = draft.assignments
        trafficFrom = draft.trafficFrom
        trafficTo = draft.trafficTo
        trafficSummary = draft.trafficSummary
        trafficClassification = draft.trafficClassification
        netMessage = draft.netMessage
    }

    private func persistDraft() {
        guard let appSettings else { return }
        let draft = NetControlDraft(
            netName: netName,
            frequency: frequency,
            activeSince: activeSince,
            netSessionID: netSessionID,
            callsign: callsign,
            operatorName: operatorName,
            location: location,
            lateCheckIn: lateCheckIn,
            checkInClassification: checkInClassification,
            checkIns: checkIns,
            traffic: traffic,
            assignment: assignment,
            assignments: assignments,
            trafficFrom: trafficFrom,
            trafficTo: trafficTo,
            trafficSummary: trafficSummary,
            trafficClassification: trafficClassification,
            netMessage: netMessage
        )
        if let data = try? JSONEncoder().encode(draft) {
            appSettings.netDraftJSON = String(data: data, encoding: .utf8)
            appSettings.updatedAt = Date()
            try? modelContext.save()
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
