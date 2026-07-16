import SwiftData
import SwiftUI

struct StationProfileView: View {
    @Query private var profiles: [StationProfile]

    private var activeProfile: StationProfile? {
        profiles.first { $0.isActive && !$0.isTombstoned } ?? profiles.first { !$0.isTombstoned }
    }

    var body: some View {
        Form {
            if let profile = activeProfile {
                Section("Profile") {
                    detail("Display Name", profile.displayName)
                    detail("Type", profile.profileType)
                    detail("Authority", profile.projectionSource)
                    detail("Canonical ID", profile.canonicalID)
                }

                Section("Operator") {
                    detail("Operator Callsign", profile.operatorCallsign)
                    detail("Station Callsign", profile.stationCallsign)
                }

                Section("Default Location") {
                    detail("Grid Square", profile.defaultGridSquare)
                    detail("QTH", profile.defaultQTH)
                    detail("State", profile.defaultState)
                    detail("Country", profile.defaultCountry)
                    detail("Power Watts", "\(String(format: "%.0f", profile.defaultPowerWatts))")
                }

                Section("Management") {
                    NavigationLink("All Stations and Equipment", destination: StationManagementView())
                }
            } else {
                ContentUnavailableView("No station projection", systemImage: "antenna.radiowaves.left.and.right")
                Section("Management") {
                    NavigationLink("Create Station", destination: StationManagementView())
                }
            }
        }
        .navigationTitle("Station Profile")
    }

    private func detail(_ title: String, _ value: String) -> some View {
        HStack(alignment: .top) {
            Text(title)
            Spacer()
            Text(value.isEmpty ? "Not set" : value)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.trailing)
        }
    }
}
