import SwiftData
import SwiftUI

struct StationManagementView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var profiles: [StationProfile]
    @Query private var equipment: [StationEquipment]

    @State private var newProfileName = ""
    @State private var newProfileType = "portable"
    @State private var newEquipmentName = ""
    @State private var newEquipmentType = "radio"
    @State private var mutationMessage: String?
    @State private var isMutating = false

    private let profileTypes = ["home", "portable", "vehicle"]
    private let equipmentTypes = ["radio", "antenna", "amplifier", "tuner", "rotor", "interface", "power_supply", "accessory"]
    private var visibleProfiles: [StationProfile] { profiles.filter { !$0.isTombstoned } }
    private var visibleEquipment: [StationEquipment] { equipment.filter { !$0.isTombstoned } }

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

                VStack(alignment: .leading, spacing: 8) {
                    TextField("Profile name", text: $newProfileName)
                    Picker("Type", selection: $newProfileType) {
                        ForEach(profileTypes, id: \.self) { type in
                            Text(type.capitalized).tag(type)
                        }
                    }
                    Button("Add Profile") {
                        Task { await addProfile() }
                    }
                    .disabled(isMutating || newProfileName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
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

                VStack(alignment: .leading, spacing: 8) {
                    TextField("Equipment name", text: $newEquipmentName)
                    Picker("Type", selection: $newEquipmentType) {
                        ForEach(equipmentTypes, id: \.self) { type in
                            Text(type.replacingOccurrences(of: "_", with: " ").capitalized).tag(type)
                        }
                    }
                    Button("Add Equipment") {
                        Task { await addEquipment() }
                    }
                    .disabled(isMutating || newEquipmentName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
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

    private func addProfile() async {
        isMutating = true
        defer { isMutating = false }
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let result = try await bridge.createStationProfile(stationProfileRequest(
                appSupportDir: supportURL.path,
                profileID: UUID().uuidString,
                displayName: newProfileName.trimmingCharacters(in: .whitespacesAndNewlines),
                profileType: newProfileType,
                stationCallsign: newProfileType == "portable" ? "KE8YGW/P" : "KE8YGW",
                operatorCallsign: "KE8YGW",
                defaultPowerWatts: newProfileType == "home" ? 100 : 10,
                active: visibleProfiles.isEmpty
            ))
            try ProjectionRefreshService.rebuildStationBook(
                from: result.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            newProfileName = ""
            await bridge.refreshStationBook()
            mutationMessage = nil
        } catch {
            mutationMessage = error.localizedDescription
        }
    }

    private func addEquipment() async {
        isMutating = true
        defer { isMutating = false }
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            let result = try await bridge.createStationEquipment(StationEquipmentMutationRequest(
                appSupportDir: supportURL.path,
                equipmentId: UUID().uuidString,
                equipmentType: newEquipmentType,
                displayName: newEquipmentName.trimmingCharacters(in: .whitespacesAndNewlines),
                manufacturer: nil,
                model: nil,
                serialNumber: nil,
                capabilities: [],
                notes: nil
            ))
            try ProjectionRefreshService.rebuildStationBook(
                from: result.stationBook,
                profiles: profiles,
                equipment: equipment,
                modelContext: modelContext
            )
            newEquipmentName = ""
            await bridge.refreshStationBook()
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
        defaultPowerWatts: Int,
        active: Bool
    ) -> StationProfileMutationRequest {
        StationProfileMutationRequest(
            appSupportDir: appSupportDir,
            stationProfileId: profileID,
            displayName: displayName,
            stationCallsign: stationCallsign,
            operatorCallsign: operatorCallsign,
            profileType: profileType,
            defaultGrid: nil,
            defaultQth: nil,
            defaultPowerWatts: defaultPowerWatts,
            notes: nil,
            active: active
        )
    }
}
