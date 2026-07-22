import SwiftUI

struct SyncWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @StateObject private var connectivity = ConnectivityMonitor()
    @State private var backgroundSync = true
    @State private var retryAutomatically = true
    @State private var syncMessage: String?

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

            Section("Actions") {
                Button("Sync Now") {
                    Task { await planRetry(markSending: false) }
                }
                Button("Retry Pending Uploads") {
                    Task { await planRetry(markSending: false) }
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

    private func recoverQueue() async {
        do {
            let result = try await bridge.recoverOfflineQueue()
            syncMessage = "Recovered \(result.recoveredCount ?? result.recovery?.recoveredInterruptedWrites ?? 0) interrupted sync operations."
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
}
