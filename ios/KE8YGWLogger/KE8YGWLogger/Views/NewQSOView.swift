import SwiftData
import SwiftUI

struct NewQSOView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    @Query private var profiles: [StationProfile]
    @Query private var settings: [AppSettings]

    @State private var callsign = ""
    @State private var contactDate = Date()
    @State private var band = "20m"
    @State private var mode = "SSB"
    @State private var frequencyMHz = ""
    @State private var rstSent = "59"
    @State private var rstReceived = "59"
    @State private var gridSquare = ""
    @State private var name = ""
    @State private var qth = ""
    @State private var state = ""
    @State private var country = ""
    @State private var notes = ""
    @State private var validationMessage: String?

    private var profile: StationProfile? { profiles.first }
    private var appSettings: AppSettings? { settings.first }

    var body: some View {
        Form {
            Section("Contact") {
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
                TextField("Frequency MHz", text: $frequencyMHz)
                    .keyboardType(.decimalPad)
            }

            Section("Reports") {
                TextField("RST Sent", text: $rstSent)
                    .keyboardType(.numberPad)
                TextField("RST Received", text: $rstReceived)
                    .keyboardType(.numberPad)
            }

            Section("Details") {
                TextField("Grid Square", text: $gridSquare)
                    .textInputAutocapitalization(.characters)
                TextField("Name", text: $name)
                TextField("QTH", text: $qth)
                TextField("State", text: $state)
                    .textInputAutocapitalization(.characters)
                TextField("Country", text: $country)
                TextField("Notes", text: $notes, axis: .vertical)
                    .lineLimit(3...6)
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
                Button("Save", action: saveQSO)
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
        gridSquare = profile?.defaultGridSquare ?? ""
        qth = profile?.defaultQTH ?? ""
        state = profile?.defaultState ?? ""
        country = profile?.defaultCountry ?? "United States"
    }

    private func saveQSO() {
        let normalizedCallsign = appSettings?.autoUppercaseCallsigns == false
            ? callsign.trimmingCharacters(in: .whitespacesAndNewlines)
            : HamRadioUtilities.normalizeCallsign(callsign)

        guard !normalizedCallsign.isEmpty else {
            validationMessage = "Callsign is required."
            return
        }
        guard HamRadioUtilities.isValidCallsign(normalizedCallsign) else {
            validationMessage = "Enter a valid amateur radio callsign."
            return
        }

        let parsedFrequency = Double(frequencyMHz.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0
        let qso = QSO(
            callsign: normalizedCallsign,
            contactDate: contactDate,
            band: band,
            mode: mode.uppercased(),
            frequencyMHz: parsedFrequency,
            rstSent: rstSent,
            rstReceived: rstReceived,
            operatorCallsign: profile?.operatorCallsign ?? "KE8YGW",
            stationCallsign: profile?.stationCallsign ?? "KE8YGW",
            gridSquare: gridSquare.uppercased(),
            name: name,
            qth: qth,
            state: state.uppercased(),
            country: country,
            notes: notes
        )

        modelContext.insert(qso)
        try? modelContext.save()
        dismiss()
    }
}
