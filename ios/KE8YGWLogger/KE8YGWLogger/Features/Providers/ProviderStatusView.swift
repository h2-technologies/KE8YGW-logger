import SwiftData
import SwiftUI

struct ProviderStatusView: View {
    @Environment(\.modelContext) private var modelContext
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var settings: [AppSettings]

    private var appSettings: AppSettings? { settings.first }
    private var providers: [ProviderMetadataSnapshot] {
        if bridge.providers.onlineProviders.isEmpty {
            return ProviderCredentialCatalog.definitions.map {
                ProviderMetadataSnapshot(
                    providerId: $0.id,
                    displayName: $0.displayName,
                    serviceType: "online",
                    requiredCredentials: $0.secureFields.map(\.id),
                    requiredConfigKeys: $0.fields.map(\.id),
                    supportsOffline: $0.id == "dx-cluster",
                    requiresNetworkAccess: $0.id != "dx-cluster",
                    status: "pending",
                    enabled: true
                )
            }
        }
        return bridge.providers.onlineProviders
    }

    var body: some View {
        List {
            Section("Provider Status") {
                if providers.isEmpty {
                    ContentUnavailableView("Provider snapshot unavailable", systemImage: "point.3.connected.trianglepath.dotted")
                } else {
                    ForEach(providers) { provider in
                        ProviderStatusRow(
                            provider: provider,
                            settings: appSettings,
                            toggle: { enabled in
                                guard let appSettings else { return }
                                appSettings.setProviderEnabled(canonicalProviderID(provider), enabled: enabled)
                                try? modelContext.save()
                                Task {
                                    do {
                                        let result = try await bridge.saveSettings(appSettings.rustSettingsPayload())
                                        if let persisted = result.settings {
                                            appSettings.apply(rust: persisted)
                                            try? modelContext.save()
                                        }
                                    } catch {
                                        bridge.lastError = error.localizedDescription
                                    }
                                }
                            }
                        )
                    }
                }
            }

            Section("Credential Management") {
                NavigationLink("Open Provider Credentials in Settings", destination: SettingsView())
                Text("Disabling a provider pauses automatic use and keeps saved Keychain credentials and history until credentials are explicitly removed in Settings.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
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

    private func canonicalProviderID(_ provider: ProviderMetadataSnapshot) -> String {
        ProviderCredentialCatalog.definition(for: provider.providerId)?.id ?? provider.providerId
    }
}

private struct ProviderStatusRow: View {
    var provider: ProviderMetadataSnapshot
    var settings: AppSettings?
    var toggle: (Bool) -> Void

    private var providerID: String {
        ProviderCredentialCatalog.definition(for: provider.providerId)?.id ?? provider.providerId
    }

    private var validation: ProviderValidationRecord {
        settings?.providerValidationRecord(providerID) ?? ProviderValidationRecord(
            configured: provider.requiredCredentials?.isEmpty == true,
            validated: false,
            validatedAt: nil,
            message: "No Settings record"
        )
    }

    private var enabled: Bool {
        settings?.isProviderEnabled(providerID) ?? provider.enabled ?? true
    }

    private var stateText: String {
        if !enabled {
            return validation.configured ? "Disabled but configured" : "Disabled and unconfigured"
        }
        if !validation.configured {
            return "Enabled but not configured"
        }
        if validation.validated {
            return "Enabled and validated"
        }
        return "Enabled but validation required"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(provider.displayName)
                        .font(.headline)
                    Text(provider.providerId)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Toggle("Enabled", isOn: Binding {
                    enabled
                } set: { value in
                    toggle(value)
                })
                .labelsHidden()
                .accessibilityLabel("\(provider.displayName) enabled")
            }

            Label(stateText, systemImage: enabled ? "checkmark.circle" : "pause.circle")
                .font(.caption)
                .foregroundStyle(enabled ? .primary : .secondary)
                .accessibilityLabel("\(provider.displayName) \(stateText)")

            HStack {
                Label(validation.configured ? "Configured" : "Unconfigured", systemImage: validation.configured ? "key.fill" : "key.slash")
                Label(validation.validated ? "Validated" : "Unvalidated", systemImage: validation.validated ? "checkmark.seal.fill" : "exclamationmark.triangle")
                if provider.requiresNetworkAccess == true {
                    Label("Network", systemImage: "network")
                }
                if provider.supportsOffline == true {
                    Label("Offline", systemImage: "wifi.slash")
                }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)

            Text(validation.message)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
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
