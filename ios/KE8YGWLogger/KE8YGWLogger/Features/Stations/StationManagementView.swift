import SwiftData
import SwiftUI

struct StationManagementView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]
    @Query private var settings: [AppSettings]

    @State private var showingProfileForm = false
    @State private var showingEquipmentForm = false
    @State private var mutationMessage: String?
    @State private var isMutating = false

    private let profileTypes = ["home", "portable", "vehicle"]
    private let equipmentTypes = ["radio", "antenna", "amplifier", "tuner", "rotor", "interface", "power_supply", "accessory"]
    private var visibleProfiles: [StationProfile] { profiles.filter { !$0.isTombstoned } }
    private var visibleEquipment: [StationEquipment] { equipment.filter { !$0.isTombstoned } }
    private var appSettings: AppSettings? { settings.first }

    var body: some View {
        List {
            Section("Active Station") {
                if let active = visibleProfiles.first(where: { $0.isActive }) ?? visibleProfiles.first {
                    DetailRow(title: "Name", value: active.displayName)
                    DetailRow(title: "Operator", value: active.operatorCallsign)
                    DetailRow(title: "Station", value: active.stationCallsign)
                    DetailRow(title: "Grid", value: active.defaultGridSquare)
                    DetailRow(title: "Power", value: "\(String(format: "%.0f", active.defaultPowerWatts)) W")
                } else {
                    ContentUnavailableView("No station profiles", systemImage: "antenna.radiowaves.left.and.right")
                }
            }

            Section("Profiles") {
                ForEach(visibleProfiles) { profile in
                    Button {
                        Task { await select(profile) }
                    } label: {
                        HStack {
                            VStack(alignment: .leading) {
                                Text(profile.displayName)
                                Text("\(profile.profileType.capitalized) - \(profile.stationCallsign)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if profile.isActive {
                                Image(systemName: "checkmark.circle.fill")
                                    .foregroundStyle(.green)
                            }
                        }
                    }
                    .disabled(isMutating)
                }

                Button {
                    showingProfileForm = true
                } label: {
                    Label("Add Profile", systemImage: "plus.circle")
                }
                .disabled(isMutating)
            }

            Section("Equipment") {
                ForEach(visibleEquipment) { item in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.displayName.isEmpty ? item.equipmentType.capitalized : item.displayName)
                            .font(.headline)
                        Text("\(item.equipmentType.replacingOccurrences(of: "_", with: " ").capitalized) - \(item.status.capitalized)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        if !item.capabilities.isEmpty {
                            Text(item.capabilities)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                Button {
                    showingEquipmentForm = true
                } label: {
                    Label("Add Equipment", systemImage: "plus.circle")
                }
                .disabled(isMutating)
            }

            Section("Rust Station Book") {
                DetailRow(title: "Profiles", value: "\(bridge.stationBook.profiles.count)")
                DetailRow(title: "Equipment", value: "\(bridge.stationBook.equipment.count)")
                DetailRow(title: "Configurations", value: "\(bridge.stationBook.configurations.count)")
                Button("Refresh Rust Snapshot") {
                    Task { await refreshStationProjection() }
                }
            }

            if let mutationMessage {
                Section {
                    Text(mutationMessage)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Stations")
        .sheet(isPresented: $showingProfileForm) {
            NavigationStack {
                StationProfileCreationForm(
                    profileTypes: profileTypes,
                    defaultOperator: appSettings?.operatorCallsign ?? "KE8YGW",
                    defaultStation: appSettings?.stationCallsign ?? "KE8YGW",
                    defaultGrid: appSettings?.maidenheadGrid ?? "",
                    onCreate: { draft in
                        await addProfile(draft)
                    }
                )
            }
        }
        .sheet(isPresented: $showingEquipmentForm) {
            NavigationStack {
                StationEquipmentCreationForm(
                    equipmentTypes: equipmentTypes,
                    onCreate: { draft in
                        await addEquipment(draft)
                    }
                )
            }
        }
        .task {
            await refreshStationProjection()
        }
    }

    private func refreshStationProjection() async {
        await bridge.refreshStationBook()
        do {
            try ProjectionRefreshService.rebuildStationBook(
                from: bridge.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            mutationMessage = nil
        } catch {
            mutationMessage = error.localizedDescription
        }
    }

    private func select(_ profile: StationProfile) async {
        isMutating = true
        defer { isMutating = false }
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let canonicalID = profile.canonicalID.isEmpty ? profile.id.uuidString : profile.canonicalID
            let result: StationBookMutationResult
            do {
                let selected = try await bridge.selectStationProfile(SelectStationProfileBridgeRequest(
                    appSupportDir: supportURL.path,
                    stationProfileId: canonicalID
                ))
                result = selected
            } catch {
                _ = try await bridge.createStationProfile(stationProfileRequest(
                    appSupportDir: supportURL.path,
                    profileID: canonicalID,
                    displayName: profile.displayName,
                    profileType: profile.profileType,
                    stationCallsign: profile.stationCallsign,
                    operatorCallsign: profile.operatorCallsign,
                    defaultPowerWatts: Int(profile.defaultPowerWatts),
                    active: true
                ))
                let selected = try await bridge.selectStationProfile(SelectStationProfileBridgeRequest(
                    appSupportDir: supportURL.path,
                    stationProfileId: canonicalID
                ))
                result = selected
            }
            try ProjectionRefreshService.rebuildStationBook(
                from: result.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            await bridge.refreshStationBook()
            mutationMessage = nil
        } catch {
            mutationMessage = error.localizedDescription
        }
    }

    private func addProfile(_ draft: StationProfileDraft) async {
        isMutating = true
        defer { isMutating = false }
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let profileID = UUID().uuidString
            let result = try await bridge.createStationProfile(stationProfileRequest(
                appSupportDir: supportURL.path,
                profileID: profileID,
                displayName: draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines),
                profileType: draft.profileType,
                stationCallsign: draft.stationCallsign.trimmingCharacters(in: .whitespacesAndNewlines).uppercased(),
                operatorCallsign: draft.operatorCallsign.trimmingCharacters(in: .whitespacesAndNewlines).uppercased(),
                defaultGrid: HamRadioUtilities.normalizedMaidenhead(draft.defaultGrid) ?? draft.defaultGrid.trimmingCharacters(in: .whitespacesAndNewlines).uppercased(),
                defaultQth: draft.defaultQTH,
                defaultPowerWatts: Int(draft.defaultPowerWatts.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0,
                notes: draft.notes,
                active: draft.active
            ))
            try ProjectionRefreshService.rebuildStationBook(
                from: result.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            if draft.active {
                let selected = try await bridge.selectStationProfile(SelectStationProfileBridgeRequest(
                    appSupportDir: supportURL.path,
                    stationProfileId: profileID
                ))
                try ProjectionRefreshService.rebuildStationBook(
                    from: selected.stationBook,
                    profiles: profiles,
                    equipment: equipment,
                    modelContext: modelContext
                )
            }
            await bridge.refreshStationBook()
            showingProfileForm = false
            mutationMessage = nil
        } catch {
            mutationMessage = error.localizedDescription
        }
    }

    private func addEquipment(_ draft: StationEquipmentDraft) async {
        isMutating = true
        defer { isMutating = false }
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let result = try await bridge.createStationEquipment(StationEquipmentMutationRequest(
                appSupportDir: supportURL.path,
                equipmentId: UUID().uuidString,
                equipmentType: draft.equipmentType,
                displayName: draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines),
                manufacturer: draft.manufacturer.trimmingCharacters(in: .whitespacesAndNewlines),
                model: draft.model.trimmingCharacters(in: .whitespacesAndNewlines),
                serialNumber: draft.serialNumber.trimmingCharacters(in: .whitespacesAndNewlines),
                capabilities: draft.capabilities
                    .split(separator: ",")
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty },
                notes: draft.notes
            ))
            try ProjectionRefreshService.rebuildStationBook(
                from: result.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            await bridge.refreshStationBook()
            showingEquipmentForm = false
            mutationMessage = nil
        } catch {
            mutationMessage = error.localizedDescription
        }
    }

    private func stationProfileRequest(
        appSupportDir: String,
        profileID: String,
        displayName: String,
        profileType: String,
        stationCallsign: String,
        operatorCallsign: String,
        defaultGrid: String? = nil,
        defaultQth: String? = nil,
        defaultPowerWatts: Int,
        notes: String? = nil,
        active: Bool
    ) -> StationProfileMutationRequest {
        StationProfileMutationRequest(
            appSupportDir: appSupportDir,
            stationProfileId: profileID,
            displayName: displayName,
            stationCallsign: stationCallsign,
            operatorCallsign: operatorCallsign,
            profileType: profileType,
            defaultGrid: defaultGrid,
            defaultQth: defaultQth,
            defaultPowerWatts: defaultPowerWatts,
            notes: notes,
            active: active
        )
    }
}

struct StationProfileDraft {
    var displayName = ""
    var profileType = "portable"
    var stationCallsign = ""
    var operatorCallsign = ""
    var defaultGrid = ""
    var defaultQTH = ""
    var defaultPowerWatts = "10"
    var notes = ""
    var active = true
}

struct StationEquipmentDraft {
    var equipmentType = "radio"
    var displayName = ""
    var manufacturer = ""
    var model = ""
    var serialNumber = ""
    var capabilities = ""
    var notes = ""
}

private struct StationProfileCreationForm: View {
    @Environment(\.dismiss) private var dismiss
    let profileTypes: [String]
    let defaultOperator: String
    let defaultStation: String
    let defaultGrid: String
    var onCreate: (StationProfileDraft) async -> Void

    @State private var draft = StationProfileDraft()
    @State private var validationMessage: String?
    @State private var isSaving = false

    var body: some View {
        Form {
            Section("Profile") {
                TextField("Display Name", text: $draft.displayName)
                Picker("Profile Type", selection: $draft.profileType) {
                    ForEach(profileTypes, id: \.self) { type in
                        Text(type.capitalized).tag(type)
                    }
                }
                TextField("Station Callsign", text: $draft.stationCallsign)
                    .textInputAutocapitalization(.characters)
                TextField("Operator Callsign", text: $draft.operatorCallsign)
                    .textInputAutocapitalization(.characters)
                Toggle("Select After Creation", isOn: $draft.active)
            }

            Section("Defaults") {
                TextField("Maidenhead Grid", text: $draft.defaultGrid)
                    .textInputAutocapitalization(.characters)
                TextField("QTH", text: $draft.defaultQTH)
                TextField("Power Watts", text: $draft.defaultPowerWatts)
                    .keyboardType(.numberPad)
                TextField("Notes", text: $draft.notes, axis: .vertical)
                    .lineLimit(2...5)
            }

            if let validationMessage {
                Section {
                    Text(validationMessage)
                        .foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("New Station Profile")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(isSaving ? "Saving" : "Create") {
                    Task { await create() }
                }
                .disabled(isSaving)
            }
        }
        .onAppear {
            if draft.stationCallsign.isEmpty {
                draft.stationCallsign = defaultStation
                draft.operatorCallsign = defaultOperator
                draft.defaultGrid = defaultGrid
                draft.active = true
            }
        }
    }

    private func create() async {
        let name = draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else {
            validationMessage = "Display name is required."
            return
        }
        guard HamRadioUtilities.isValidCallsign(draft.stationCallsign) else {
            validationMessage = "A valid station callsign is required."
            return
        }
        guard HamRadioUtilities.isValidCallsign(draft.operatorCallsign) else {
            validationMessage = "A valid operator callsign is required."
            return
        }
        if !draft.defaultGrid.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
           HamRadioUtilities.normalizedMaidenhead(draft.defaultGrid) == nil {
            validationMessage = "Enter a valid 4- or 6-character Maidenhead grid."
            return
        }
        isSaving = true
        defer { isSaving = false }
        await onCreate(draft)
    }
}

private struct StationEquipmentCreationForm: View {
    @Environment(\.dismiss) private var dismiss
    let equipmentTypes: [String]
    var onCreate: (StationEquipmentDraft) async -> Void

    @State private var draft = StationEquipmentDraft()
    @State private var validationMessage: String?
    @State private var isSaving = false

    var body: some View {
        Form {
            Section("Equipment") {
                Picker("Type", selection: $draft.equipmentType) {
                    ForEach(equipmentTypes, id: \.self) { type in
                        Text(type.replacingOccurrences(of: "_", with: " ").capitalized).tag(type)
                    }
                }
                TextField("Display Name", text: $draft.displayName)
                TextField("Manufacturer", text: $draft.manufacturer)
                TextField("Model", text: $draft.model)
                TextField("Serial Number", text: $draft.serialNumber)
                    .textInputAutocapitalization(.characters)
                TextField("Capabilities", text: $draft.capabilities)
                TextField("Notes", text: $draft.notes, axis: .vertical)
                    .lineLimit(2...5)
            }

            if let validationMessage {
                Section {
                    Text(validationMessage)
                        .foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("New Equipment")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(isSaving ? "Saving" : "Create") {
                    Task { await create() }
                }
                .disabled(isSaving)
            }
        }
    }

    private func create() async {
        guard !draft.displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            validationMessage = "Display name is required."
            return
        }
        isSaving = true
        defer { isSaving = false }
        await onCreate(draft)
    }
}
