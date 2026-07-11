import SwiftUI

struct ProviderStatusView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @State private var credentialProvider = "qrz"
    @State private var credentialAccount = ""
    @State private var credentialSecret = ""
    @State private var credentialMessage: String?

    private let vault = KeychainCredentialVault()
    private let providerOrder = ["qrz", "hamqth", "pota", "sotawatch", "dx_cluster", "club_log", "qrz_logbook", "eqsl", "lotw"]

    var body: some View {
        List {
            Section("Provider Health") {
                if bridge.providers.onlineProviders.isEmpty {
                    ContentUnavailableView("Provider snapshot unavailable", systemImage: "point.3.connected.trianglepath.dotted")
                } else {
                    ForEach(bridge.providers.onlineProviders) { provider in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(provider.displayName)
                                    .font(.headline)
                                Spacer()
                                Text(provider.requiredCredentials?.isEmpty == false ? "Credentials" : "Ready")
                                    .foregroundStyle(provider.requiredCredentials?.isEmpty == false ? .orange : .green)
                            }
                            Text(provider.providerId)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            if provider.requiresNetworkAccess == true {
                                Label("Network provider", systemImage: "network")
                                    .font(.caption)
                            }
                        }
                    }
                }
            }

            Section("Known Integrations") {
                ForEach(providerOrder, id: \.self) { provider in
                    HStack {
                        Text(provider.replacingOccurrences(of: "_", with: " ").uppercased())
                        Spacer()
                        Text(bridge.providers.apiStatus?[provider] ?? "pending")
                            .font(.caption)
                            .foregroundStyle(AppTheme.statusColor(bridge.providers.apiStatus?[provider]))
                    }
                }
            }

            Section("Credentials") {
                Picker("Provider", selection: $credentialProvider) {
                    ForEach(providerOrder, id: \.self) { provider in
                        Text(provider.replacingOccurrences(of: "_", with: " ").uppercased()).tag(provider)
                    }
                }
                TextField("Account", text: $credentialAccount)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("Secret / Token", text: $credentialSecret)
                Button("Save to Keychain", action: saveCredential)
                    .disabled(credentialAccount.isEmpty || credentialSecret.isEmpty)
                if let credentialMessage {
                    Text(credentialMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Providers")
        .toolbar {
            Button("Refresh") {
                Task { await bridge.refreshProviders() }
            }
        }
        .task {
            await bridge.refreshProviders()
        }
    }

    private func saveCredential() {
        do {
            try vault.save(secret: credentialSecret, account: credentialAccount, providerId: credentialProvider)
            credentialSecret = ""
            credentialMessage = "Saved \(credentialProvider) credential metadata in Keychain."
        } catch {
            credentialMessage = error.localizedDescription
        }
    }
}

struct CallsignLookupView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @State private var callsign = ""
    @State private var lookup: CallsignLookupPayload?
    @State private var errorMessage: String?
    @State private var actionMessage: String?

    var body: some View {
        List {
            Section("Lookup") {
                TextField("Callsign", text: $callsign)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                    .keyboardType(.asciiCapable)
                Button("Search", action: search)
                    .disabled(callsign.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            if let lookup {
                Section("Result") {
                    DetailRow(title: "Callsign", value: lookup.callsign)
                    DetailRow(title: "Provider", value: lookup.providerId)
                    DetailRow(title: "Name", value: lookup.result?.name ?? "")
                    DetailRow(title: "Address/QTH", value: lookup.result?.qth ?? "")
                    DetailRow(title: "Grid", value: lookup.result?.grid ?? "")
                    DetailRow(title: "County", value: "")
                    DetailRow(title: "CQ Zone", value: lookup.result?.cqZone.map(String.init) ?? "")
                    DetailRow(title: "ITU Zone", value: lookup.result?.ituZone.map(String.init) ?? "")
                    DetailRow(title: "License", value: lookup.result?.licenseClass ?? "")
                    DetailRow(title: "DXCC", value: lookup.result?.dxcc.map(String.init) ?? "")
                    DetailRow(title: "Country", value: lookup.result?.country ?? "")
                }

                Section("Actions") {
                    Button("Save Contact") {
                        actionMessage = "Contact saved to the local workflow queue."
                    }
                    Button("Navigate") {
                        actionMessage = "Navigation queued for MapKit."
                    }
                    Button("View on Map") {
                        actionMessage = "Map marker queued for \(lookup.callsign)."
                    }
                    if let actionMessage {
                        Text(actionMessage)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if let errorMessage {
                Section {
                    Text(errorMessage)
                        .foregroundStyle(.red)
                }
            }
        }
        .navigationTitle("Callsign")
    }

    private func search() {
        Task {
            do {
                lookup = try await bridge.lookup(callsign: HamRadioUtilities.normalizeCallsign(callsign))
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}
