import SwiftData
import SwiftUI

struct StationProfileView: View {
    @Environment(\.modelContext) private var modelContext
    @Query private var profiles: [StationProfile]

    var body: some View {
        Form {
            if let profile = profiles.first {
                Section("Operator") {
                    TextField("Operator Callsign", text: bind(profile, \.operatorCallsign))
                        .textInputAutocapitalization(.characters)
                    TextField("Station Callsign", text: bind(profile, \.stationCallsign))
                        .textInputAutocapitalization(.characters)
                }

                Section("Default Location") {
                    TextField("Grid Square", text: bind(profile, \.defaultGridSquare))
                        .textInputAutocapitalization(.characters)
                    TextField("QTH", text: bind(profile, \.defaultQTH))
                    TextField("State", text: bind(profile, \.defaultState))
                        .textInputAutocapitalization(.characters)
                    TextField("Country", text: bind(profile, \.defaultCountry))
                }

                Section("Future") {
                    Text("TODO: Maidenhead grid calculation from GPS.")
                    Text("TODO: Station equipment profiles and portable station presets.")
                }
                .foregroundStyle(.secondary)
            } else {
                Button("Create Default Profile") {
                    modelContext.insert(StationProfile())
                    try? modelContext.save()
                }
            }
        }
        .navigationTitle("Station Profile")
    }

    private func bind(_ profile: StationProfile, _ keyPath: ReferenceWritableKeyPath<StationProfile, String>) -> Binding<String> {
        Binding {
            profile[keyPath: keyPath]
        } set: { value in
            profile[keyPath: keyPath] = value
            profile.updatedAt = Date()
            try? modelContext.save()
        }
    }
}
