import SwiftData
import SwiftUI

struct SyncWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @Query private var settings: [AppSettings]
    @StateObject private var connectivity = ConnectivityMonitor()
    @State private var backgroundSync = true
    @State private var retryAutomatically = true
    @State private var syncMessage: String?
    @State private var lanPeerDeviceId = ""
    @State private var lanPeerDisplayName = ""
    @State private var lanPairingTokenId = ""
    @State private var lanPairingFingerprint = ""
    @State private var lanSelectedDeviceId = ""
    @State private var lanIssuedPairing: SyncIssuedPairingToken?
    private let credentialVault = KeychainCredentialVault()

    var body: some View {
        List {
            Section("Hosted Sync") {
                DetailRow(title: "Connection", value: bridge.sync.cloudConnectionState ?? "disconnected")
                DetailRow(title: "Network", value: connectivity.state.label)
                DetailRow(title: "Pending Changes", value: "\(bridge.sync.pendingChanges ?? 0)")
                if let health = bridge.sync.offlineQueue?.health {
                    DetailRow(title: "Ready", value: "\(health.readyToSend ?? 0)")
                    DetailRow(title: "Needs Review", value: "\(health.userActionRequired ?? 0)")
                }
                if let reviewHealth = bridge.sync.conflictReviews?.health {
                    DetailRow(title: "Open Reviews", value: "\(reviewHealth.open ?? 0)")
                }
                DetailRow(title: "Conflicts", value: "\(bridge.sync.conflicts?.count ?? 0)")
                Toggle("Background Sync", isOn: $backgroundSync)
                Toggle("Automatic Retry", isOn: $retryAutomatically)
            }

            Section("Offline Queue") {
                let mutations = bridge.sync.offlineQueue?.mutations ?? []
                if mutations.isEmpty {
                    Text("No queued offline changes.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(mutations) { mutation in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(mutation.operationType ?? "sync mutation")
                                .font(.headline)
                            HStack {
                                Text((mutation.status ?? "unknown").replacingOccurrences(of: "_", with: " ").capitalized)
                                if let lastErrorCode = mutation.lastErrorCode {
                                    Text(lastErrorCode)
                                }
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            Section("Merge Status") {
                let reviews = bridge.sync.conflictReviews?.openReviews ?? []
                if reviews.isEmpty && bridge.sync.conflicts?.isEmpty != false {
                    Label("No conflicts", systemImage: "checkmark.circle")
                        .foregroundStyle(.green)
                } else {
                    ForEach(reviews) { review in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(review.report?.statusLabel ?? review.statusLabel)
                                .font(.headline)
                            Text(review.report?.recommendedAction ?? "Manual review required before syncing.")
                                .font(.subheadline)
                                .foregroundStyle(.orange)
                            if let peerID = review.report?.peerId {
                                Text(peerID)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            ForEach(review.report?.conflicts ?? []) { conflict in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(conflict.kindLabel)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    Text(conflict.message ?? "Conflict requires operator review.")
                                        .font(.caption)
                                }
                            }
                        }
                    }
                    ForEach(bridge.sync.conflicts ?? [], id: \.self) { conflict in
                        Text(conflict)
                            .foregroundStyle(.orange)
                    }
                }
            }

            Section("LAN Trust") {
                let trust = bridge.sync.lanTrust
                DetailRow(title: "Trusted Devices", value: "\(trust?.activeTrustedDevices.count ?? 0)")
                DetailRow(title: "Pairing Codes", value: "\(trust?.activePairingTokens.count ?? 0)")
                if let error = bridge.sync.lanTrustError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                if let lanIssuedPairing {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(lanIssuedPairing.pairingCode)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                        Text("Expires \(lanIssuedPairing.expiresAt)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                ForEach(trust?.trustedDevices ?? []) { device in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(device.displayName ?? "LAN Peer")
                            Spacer()
                            Text(device.statusLabel)
                                .font(.caption)
                                .foregroundStyle(device.revokedAt == nil ? .green : .secondary)
                        }
                        Text(device.deviceId ?? "unknown device")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                        if let authCredentialId = device.authCredentialId {
                            Text("Credential \(authCredentialId)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                        }
                    }
                    .contentShape(Rectangle())
                    .onTapGesture {
                        lanSelectedDeviceId = device.deviceId ?? ""
                    }
                }
                TextField("Peer Device ID", text: $lanPeerDeviceId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Peer Name", text: $lanPeerDisplayName)
                TextField("Pairing Token ID", text: $lanPairingTokenId)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                TextField("Fingerprint", text: $lanPairingFingerprint)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                HStack {
                    Button("Issue Code") {
                        Task { await issueLanPairingToken() }
                    }
                    Button("Trust Peer") {
                        Task { await trustLanPeer() }
                    }
                }
                HStack {
                    Button("Rotate Auth") {
                        Task { await rotateLanAuth() }
                    }
                    .disabled(selectedLanDeviceId() == nil)
                    Button("Revoke") {
                        Task { await revokeLanPeer() }
                    }
                    .disabled(selectedLanDeviceId() == nil)
                }
            }

            Section("Actions") {
                Button("Sync Now") {
                    Task { await executeRetryPush() }
                }
                Button("Pull Missing") {
                    Task { await executePull() }
                }
                Button("Retry Pending Uploads") {
                    Task { await executeRetryPush() }
                }
                .disabled(!retryAutomatically)
                Button("Recover Queue") {
                    Task { await recoverQueue() }
                }
                Button("Refresh") {
                    Task { await bridge.refreshSync() }
                }
                if let syncMessage {
                    Text(syncMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Sync")
        .task {
            connectivity.start()
            await bridge.refreshSync()
        }
    }

    private func issueLanPairingToken() async {
        do {
            let result = try await bridge.issueLanPairingToken(
                issuerDisplayName: "KE8YGW Logger iOS",
                approvedByOperator: true
            )
            lanIssuedPairing = result.pairing
            syncMessage = "Issued LAN pairing code \(result.pairing.tokenId)."
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func trustLanPeer() async {
        let peerDeviceId = lanPeerDeviceId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard UUID(uuidString: peerDeviceId) != nil else {
            syncMessage = "Enter a valid peer device UUID."
            return
        }
        let displayName = lanPeerDisplayName
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty ?? "LAN Peer"
        let credentialId = UUID().uuidString
        do {
            try credentialVault.save(
                secret: generateLanAuthSecret(),
                account: lanAuthAccount(credentialId),
                providerId: "lan_sync"
            )
            let result = try await bridge.trustLanPeer(
                peerDeviceId: peerDeviceId,
                peerDisplayName: displayName,
                pairingTokenId: lanPairingTokenId.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty,
                publicKeyFingerprint: lanPairingFingerprint.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty,
                authCredentialId: credentialId
            )
            lanSelectedDeviceId = result.trustedDevice.deviceId ?? peerDeviceId
            lanPeerDeviceId = ""
            lanPeerDisplayName = ""
            syncMessage = "Trusted \(result.trustedDevice.displayName ?? displayName) for LAN sync."
        } catch {
            try? credentialVault.delete(
                account: lanAuthAccount(credentialId),
                providerId: "lan_sync"
            )
            syncMessage = error.localizedDescription
        }
    }

    private func rotateLanAuth() async {
        guard let deviceId = selectedLanDeviceId() else {
            syncMessage = "Select a trusted LAN device first."
            return
        }
        let credentialId = UUID().uuidString
        do {
            try credentialVault.save(
                secret: generateLanAuthSecret(),
                account: lanAuthAccount(credentialId),
                providerId: "lan_sync"
            )
            let result = try await bridge.rotateLanAuthCredential(
                deviceId: deviceId,
                newAuthCredentialId: credentialId
            )
            if let previous = result.rotation.previousAuthCredentialId {
                try? credentialVault.delete(
                    account: lanAuthAccount(previous),
                    providerId: "lan_sync"
                )
            }
            lanSelectedDeviceId = result.rotation.trustedDevice.deviceId ?? deviceId
            syncMessage = "Rotated LAN auth credential."
        } catch {
            try? credentialVault.delete(
                account: lanAuthAccount(credentialId),
                providerId: "lan_sync"
            )
            syncMessage = error.localizedDescription
        }
    }

    private func revokeLanPeer() async {
        guard let deviceId = selectedLanDeviceId() else {
            syncMessage = "Select a trusted LAN device first."
            return
        }
        do {
            let result = try await bridge.revokeLanPeer(deviceId: deviceId)
            if let credentialId = result.trustedDevice.authCredentialId {
                try? credentialVault.delete(
                    account: lanAuthAccount(credentialId),
                    providerId: "lan_sync"
                )
            }
            lanSelectedDeviceId = ""
            syncMessage = "Revoked LAN trust for \(result.trustedDevice.displayName ?? "peer")."
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func recoverQueue() async {
        do {
            let result = try await bridge.recoverOfflineQueue()
            syncMessage = "Recovered \(result.recoveredCount ?? result.recovery?.recoveredInterruptedWrites ?? 0) interrupted sync operations."
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func executeRetryPush() async {
        guard let serverURL = syncServerURL() else {
            syncMessage = "Enter a valid sync server URL in Settings."
            return
        }
        let syncToken: String?
        do {
            syncToken = try readSyncToken()
        } catch {
            syncMessage = "Unable to read sync credentials: \(error.localizedDescription)"
            return
        }
        guard let syncToken else {
            syncMessage = "Add a sync token in Settings before pushing queued changes."
            return
        }

        do {
            let result = try await bridge.executeOfflineRetryPush(
                serverURL: serverURL,
                syncToken: syncToken,
                endpointStyle: .logbookScoped,
                maxMutations: 25,
                networkAvailable: connectivity.state.hasUsableInternet,
                backgroundTimeBudgetSeconds: 20,
                transport: SyncHTTPTransport()
            )
            syncMessage = syncResultMessage(result)
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func executePull() async {
        guard let serverURL = syncServerURL() else {
            syncMessage = "Enter a valid sync server URL in Settings."
            return
        }
        let syncToken: String?
        do {
            syncToken = try readSyncToken()
        } catch {
            syncMessage = "Unable to read sync credentials: \(error.localizedDescription)"
            return
        }
        guard let syncToken else {
            syncMessage = "Add a sync token in Settings before pulling missing events."
            return
        }

        do {
            let result = try await bridge.executeRemotePull(
                serverURL: serverURL,
                syncToken: syncToken,
                endpointStyle: .logbookScoped,
                networkAvailable: connectivity.state.hasUsableInternet,
                transport: SyncHTTPTransport()
            )
            syncMessage = syncPullResultMessage(result)
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func planRetry(markSending: Bool) async {
        do {
            let result = try await bridge.planOfflineRetry(
                maxMutations: 25,
                markSending: markSending,
                networkAvailable: connectivity.state.hasUsableInternet,
                backgroundTimeBudgetSeconds: 20
            )
            if result.retryPlan.blockedByNetwork {
                syncMessage = "Network unavailable; queued changes remain pending."
            } else if result.retryPlan.operationIds.isEmpty {
                syncMessage = "No ready offline changes."
            } else {
                syncMessage = "Prepared \(result.retryPlan.operationIds.count) queued changes and \(result.retryPlan.transportableEvents.count) event envelopes for native sync transport."
            }
        } catch {
            syncMessage = error.localizedDescription
        }
    }

    private func syncServerURL() -> URL? {
        let rawValue = (settings.first?.serverURL ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: rawValue),
              let scheme = url.scheme?.lowercased(),
              (scheme == "http" || scheme == "https"),
              url.host != nil
        else {
            return nil
        }
        return url
    }

    private func readSyncToken() throws -> String? {
        let token = try credentialVault
            .read(account: "sync_token", providerId: "sync")?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let token, !token.isEmpty else {
            return nil
        }
        return token
    }

    private func selectedLanDeviceId() -> String? {
        let selected = lanSelectedDeviceId.trimmingCharacters(in: .whitespacesAndNewlines)
        if UUID(uuidString: selected) != nil {
            return selected
        }
        return bridge.sync.lanTrust?.activeTrustedDevices
            .compactMap(\.deviceId)
            .first { UUID(uuidString: $0) != nil }
    }

    private func lanAuthAccount(_ credentialId: String) -> String {
        "lan_auth:\(credentialId)"
    }

    private func generateLanAuthSecret() -> String {
        UUID().uuidString.replacingOccurrences(of: "-", with: "")
            + UUID().uuidString.replacingOccurrences(of: "-", with: "")
    }

    private func syncPullResultMessage(_ result: SyncPullExecutionResult) -> String {
        switch result.status {
        case .blockedByNetwork:
            return "Network unavailable; missing events were not pulled."
        case .noRemoteEvents, .inSync:
            return "No missing remote events."
        case .applied:
            return "Pulled \(result.acceptedCount) remote events."
        case .diverged:
            return "Sync peer history diverged; manual review is required."
        case .rejected:
            return "Remote events were rejected by local verification."
        }
    }

    private func syncResultMessage(_ result: SyncRetryExecutionResult) -> String {
        switch result.status {
        case .blockedByNetwork:
            return "Network unavailable; queued changes remain pending."
        case .noReadyEvents:
            return "No ready offline changes."
        case .missingTransportEventsRecorded:
            return "Queued changes need review because local event envelopes are missing."
        case .accepted:
            return "Pushed \(result.acceptedOperationCount) queued changes."
        case .partialFailureRecorded:
            return "Pushed \(result.acceptedOperationCount) changes; \(result.failedOperationCount) need review."
        case .transientFailureRecorded:
            return "Sync push failed; retry was scheduled."
        case .userActionRequired:
            return "Sync push needs operator review."
        case .diverged:
            return "Sync peer history diverged; manual review is required."
        }
    }
}

private extension String {
    var nonEmpty: String? {
        isEmpty ? nil : self
    }
}
