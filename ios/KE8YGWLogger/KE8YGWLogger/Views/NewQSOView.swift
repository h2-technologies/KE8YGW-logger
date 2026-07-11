import SwiftData
import SwiftUI

struct NewQSOView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var qsos: [QSO]
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @Query private var settings: [AppSettings]

    private let qsoKinds = ["Voice", "CW", "Digital", "Satellite", "Contest", "Net", "Emergency", "POTA", "SOTA"]

    @State private var callsign = ""
    @State private var contactDate = Date()
    @State private var qsoKind = "Voice"
    @State private var band = "20m"
    @State private var mode = "SSB"
    @State private var submode = ""
    @State private var frequencyMHz = ""
    @State private var rstSent = "59"
    @State private var rstReceived = "59"
    @State private var powerWatts = ""
    @State private var gridSquare = ""
    @State private var county = ""
    @State private var name = ""
    @State private var qth = ""
    @State private var state = ""
    @State private var country = ""
    @State private var contestExchange = ""
    @State private var satelliteName = ""
    @State private var potaReferences = ""
    @State private var sotaReferences = ""
    @State private var notes = ""
    @State private var validationMessage: String?
    @State private var isSaving = false

    private var profile: StationProfile? { profiles.first { $0.isActive } ?? profiles.first }
    private var appSettings: AppSettings? { settings.first }

    var body: some View {
        Form {
            Section("Contact") {
                Picker("Type", selection: $qsoKind) {
                    ForEach(qsoKinds, id: \.self) { kind in
                        Text(kind).tag(kind)
                    }
                }
                TextField("Callsign", text: $callsign)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                    .keyboardType(.asciiCapable)
                DatePicker("Date / Time", selection: $contactDate)
                TextField("Band", text: $band)
                    .textInputAutocapitalization(.never)
                TextField("Mode", text: $mode)
                    .textInputAutocapitalization(.characters)
                    .onChange(of: mode) { _, newValue in
                        let rst = HamRadioUtilities.defaultRST(for: newValue)
                        rstSent = rst
                        rstReceived = rst
                    }
                TextField("Submode", text: $submode)
                    .textInputAutocapitalization(.characters)
                TextField("Frequency MHz", text: $frequencyMHz)
                    .keyboardType(.decimalPad)
                    .onChange(of: frequencyMHz) { _, newValue in
                        if let value = Double(newValue), let inferred = HamRadioUtilities.bandFromFrequencyMHz(value) {
                            band = inferred
                        }
                    }
            }

            Section("Reports") {
                TextField("RST Sent", text: $rstSent)
                    .keyboardType(.numberPad)
                TextField("RST Received", text: $rstReceived)
                    .keyboardType(.numberPad)
                TextField("Power Watts", text: $powerWatts)
                    .keyboardType(.decimalPad)
            }

            Section("Station") {
                Text(profile?.displayName ?? "No active station")
                    .foregroundStyle(.secondary)
                if equipment.isEmpty {
                    Text("No local equipment cache. Add radios, antennas, amplifiers, and portable kits from Stations.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(equipment.prefix(3)) { item in
                        Label(item.displayName.isEmpty ? item.equipmentType.capitalized : item.displayName, systemImage: "antenna.radiowaves.left.and.right")
                            .font(.caption)
                    }
                }
            }

            Section("Location") {
                TextField("Grid Square", text: $gridSquare)
                    .textInputAutocapitalization(.characters)
                TextField("County", text: $county)
                TextField("Name", text: $name)
                TextField("QTH", text: $qth)
                TextField("State", text: $state)
                    .textInputAutocapitalization(.characters)
                TextField("Country", text: $country)
                TextField("Notes", text: $notes, axis: .vertical)
                    .lineLimit(3...6)
            }

            Section("Specialized") {
                TextField("Contest Exchange", text: $contestExchange)
                TextField("Satellite", text: $satelliteName)
                TextField("POTA References", text: $potaReferences)
                    .textInputAutocapitalization(.characters)
                TextField("SOTA References", text: $sotaReferences)
                    .textInputAutocapitalization(.characters)
            }

            if let validationMessage {
                Section {
                    Text(validationMessage)
                        .foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("New QSO")
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button(isSaving ? "Saving" : "Save") {
                    Task { await saveQSO() }
                }
                .disabled(isSaving)
            }
        }
        .onAppear(perform: applyDefaults)
    }

    private func applyDefaults() {
        guard frequencyMHz.isEmpty else { return }
        band = appSettings?.defaultBand ?? "20m"
        mode = appSettings?.defaultMode ?? "SSB"
        let rst = HamRadioUtilities.defaultRST(for: mode)
        rstSent = rst
        rstReceived = rst
        powerWatts = String(format: "%.0f", profile?.defaultPowerWatts ?? 100)
        gridSquare = profile?.defaultGridSquare ?? ""
        qth = profile?.defaultQTH ?? ""
        state = profile?.defaultState ?? ""
        country = profile?.defaultCountry ?? "United States"
    }

    private func saveQSO() async {
        let normalizedCallsign = appSettings?.autoUppercaseCallsigns == false
            ? callsign.trimmingCharacters(in: .whitespacesAndNewlines)
            : HamRadioUtilities.normalizeCallsign(callsign)

        guard !normalizedCallsign.isEmpty else {
            validationMessage = "Callsign is required."
            return
        }

        let parsedFrequency = Double(frequencyMHz.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0
        let parsedPower = Double(powerWatts.trimmingCharacters(in: .whitespacesAndNewlines)) ?? profile?.defaultPowerWatts ?? 0
        isSaving = true
        defer { isSaving = false }

        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let operationID = UUID().uuidString
            let payload = CreateQSOBridgePayload(
                contactedCallsign: normalizedCallsign,
                stationCallsign: profile?.stationCallsign ?? "KE8YGW",
                operatorCallsign: profile?.operatorCallsign ?? "KE8YGW",
                startedAt: Self.isoFormatter.string(from: contactDate),
                mode: mode.uppercased(),
                band: band,
                submode: submode.isEmpty ? nil : submode.uppercased(),
                frequencyMhz: parsedFrequency > 0 ? parsedFrequency : nil,
                rstSent: rstSent,
                rstReceived: rstReceived,
                powerWatts: parsedPower,
                stationProfileId: profile?.canonicalID.isEmpty == false ? profile?.canonicalID : profile?.id.uuidString,
                equipmentSummary: equipment.map { $0.displayName }.filter { !$0.isEmpty }.joined(separator: ", "),
                grid: gridSquare.uppercased(),
                county: county,
                name: name,
                qth: qth,
                state: state.uppercased(),
                country: country,
                qsoKind: qsoKind.lowercased(),
                contestExchange: contestExchange,
                satelliteName: satelliteName,
                potaReferences: potaReferences.uppercased(),
                sotaReferences: sotaReferences.uppercased(),
                notes: notes,
                source: "ios/native"
            )
            let result = try await bridge.createQSO(CreateQSOBridgeRequest(
                appSupportDir: supportURL.path,
                operationId: operationID,
                deviceId: nil,
                qso: payload
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
            validationMessage = nil
            dismiss()
        } catch {
            validationMessage = error.localizedDescription
        }
    }

    private static let isoFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}
