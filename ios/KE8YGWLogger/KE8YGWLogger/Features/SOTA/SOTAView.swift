import SwiftData
import SwiftUI

struct SOTAView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var qsos: [QSO]
    @Query private var settings: [AppSettings]
    @StateObject private var connectivity = ConnectivityMonitor()
    @State private var summitReference = ""
    @State private var activationStartedAt: Date?
    @State private var activationID: String?
    @State private var spotFrequency = "14.285"
    @State private var mode = "SSB"
    @State private var spotMessage: String?
    @State private var activationMessage: String?
    @State private var offlineActivation = false
    @State private var confirmEndActivation = false

    private var activationQSOs: [QSO] {
        qsos.filter { !$0.sotaReferences.isEmpty || $0.qsoKind == "sota" }
    }
    private var appSettings: AppSettings? { settings.first }
    private var eligibility: ActivationEligibility {
        ActivationEligibility.evaluate(
            providerID: "sotawatch",
            settings: appSettings,
            networkAvailable: connectivity.state.hasUsableInternet,
            validationTTLHours: appSettings?.validationTTLHours ?? 24
        )
    }
    private var eligibilityIconName: String {
        if eligibility.offlineOnly { return "wifi.slash" }
        return eligibility.canStart ? "checkmark.seal" : "exclamationmark.triangle"
    }
    private var eligibilityTint: Color {
        eligibility.canStart ? .secondary : .orange
    }
    private var activationButtonTitle: String {
        activationStartedAt == nil ? "Start Activation" : "End Activation"
    }
    private var activationTypeText: String {
        offlineActivation ? "Offline local-only" : "Online provider-gated"
    }
    private var uniqueCallsText: String {
        "\(Set(activationQSOs.map(\.callsign)).count)"
    }
    private var bandsText: String {
        Set(activationQSOs.map(\.band)).sorted().joined(separator: ", ")
    }

    var body: some View {
        sotaContent
            .navigationTitle("SOTA")
            .task {
                connectivity.start()
                restoreDraft()
            }
            .onChange(of: summitReference) { _, _ in persistDraft() }
            .onChange(of: spotFrequency) { _, _ in persistDraft() }
            .onChange(of: mode) { _, _ in persistDraft() }
            .confirmationDialog("End this SOTA activation?", isPresented: $confirmEndActivation, titleVisibility: .visible) {
                Button("End Activation", role: .destructive) {
                    Task { await toggleActivation() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("The activation will be ended through the Rust event path and the local draft will be cleared.")
            }
    }

    private var sotaContent: some View {
        List {
            summitSection
            statisticsSection
            spottingSection
            exportSection
        }
    }

    @ViewBuilder
    private var summitSection: some View {
        Section("Summit") {
            TextField("Summit Reference", text: $summitReference)
                .textInputAutocapitalization(.characters)
            TextField("Spot Frequency MHz", text: $spotFrequency)
                .keyboardType(.decimalPad)
            TextField("Mode", text: $mode)
                .textInputAutocapitalization(.characters)
            activeActivationRows
            DetailRow(title: "Network", value: connectivity.state.label)
            activationEligibilityLabel
            activationToggleButton
            activationMessageRow
        }
    }

    @ViewBuilder
    private var activeActivationRows: some View {
        if let activationStartedAt {
            DetailRow(title: "Started", value: activationStartedAt.formatted(date: .omitted, time: .standard))
            DetailRow(title: "Elapsed", value: elapsedText(since: activationStartedAt))
            DetailRow(title: "Activation Type", value: activationTypeText)
        }
    }

    private var activationEligibilityLabel: some View {
        Label(eligibility.message, systemImage: eligibilityIconName)
            .font(.caption)
            .foregroundStyle(eligibilityTint)
            .accessibilityLabel("SOTA activation eligibility: \(eligibility.message)")
    }

    private var activationToggleButton: some View {
        Button(activationButtonTitle) {
            if activationStartedAt == nil {
                Task { await toggleActivation() }
            } else {
                confirmEndActivation = true
            }
        }
        .disabled(startDisabled)
    }

    @ViewBuilder
    private var activationMessageRow: some View {
        if let activationMessage {
            Text(activationMessage)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var statisticsSection: some View {
        Section("Statistics") {
            DetailRow(title: "QSOs", value: "\(activationQSOs.count)")
            DetailRow(title: "Unique Calls", value: uniqueCallsText)
            DetailRow(title: "Bands", value: bandsText)
        }
    }

    @ViewBuilder
    private var spottingSection: some View {
        Section("Spotting") {
            Button("Post SOTAWatch Spot") {
                spotMessage = "SOTAWatch spot queued for \(summitReference) on \(spotFrequency) MHz \(mode)."
            }
            .disabled(summitReference.isEmpty)
            if let spotMessage {
                Text(spotMessage)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Label("SOTAWatch provider status is supplied by the Rust provider bridge.", systemImage: "point.3.connected.trianglepath.dotted")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var exportSection: some View {
        Section("Export") {
            NavigationLink("Export Logs", destination: ExportView())
        }
    }

    private var startDisabled: Bool {
        if activationStartedAt != nil { return false }
        return summitReference.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !eligibility.canStart
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
                offlineActivation = false
                activationMessage = "Activation ended through Rust."
                appSettings?.sotaDraftJSON = ""
                appSettings?.updatedAt = Date()
                try? modelContext.save()
                return
            } else {
                let startedAt = Date()
                let startingOffline = eligibility.offlineOnly
                let result = try await bridge.startActivation(ActivationBridgeRequest(
                    appSupportDir: supportURL.path,
                    activationType: "sota",
                    stationCallsign: bridge.dashboard.activeStation?.stationCallsign ?? "KE8YGW",
                    operatorCallsign: bridge.dashboard.operatorCallsign,
                    startedAt: Self.isoFormatter.string(from: startedAt),
                    parkId: nil,
                    summitId: summitReference.uppercased(),
                    grid: bridge.dashboard.gps?.grid,
                    locationName: nil,
                    notes: startingOffline ? "iOS offline local-only start; provider validation skipped because NWPathMonitor reported no usable internet." : appSettings?.activationNotesTemplate
                ))
                activationID = result.officialEvent.entityId
                activationStartedAt = startedAt
                offlineActivation = startingOffline
                activationMessage = startingOffline ? "Offline local-only activation started through Rust." : "Activation started through Rust after provider validation gate."
            }
            persistDraft()
        } catch {
            activationMessage = error.localizedDescription
            persistDraft()
        }
    }

    private func restoreDraft() {
        guard let data = appSettings?.sotaDraftJSON?.data(using: .utf8),
              let draft = try? JSONDecoder().decode(ActivationDraft.self, from: data) else { return }
        summitReference = draft.reference
        activationStartedAt = draft.startedAt
        activationID = draft.activationID
        spotFrequency = draft.spotFrequency.isEmpty ? spotFrequency : draft.spotFrequency
        mode = draft.mode
        activationMessage = draft.message
        offlineActivation = draft.offlineOnly
    }

    private func persistDraft() {
        guard let appSettings else { return }
        let draft = ActivationDraft(
            reference: summitReference,
            startedAt: activationStartedAt,
            activationID: activationID,
            spotFrequency: spotFrequency,
            mode: mode,
            message: activationMessage,
            offlineOnly: offlineActivation
        )
        if let data = try? JSONEncoder().encode(draft) {
            appSettings.sotaDraftJSON = String(data: data, encoding: .utf8)
            appSettings.updatedAt = Date()
            try? modelContext.save()
        }
    }

    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
