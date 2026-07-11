import SwiftUI

struct SyncWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @State private var backgroundSync = true
    @State private var retryAutomatically = true
    @State private var syncMessage: String?

    var body: some View {
        List {
            Section("Hosted Sync") {
                DetailRow(title: "Connection", value: bridge.sync.cloudConnectionState ?? "disconnected")
                DetailRow(title: "Pending Changes", value: "\(bridge.sync.pendingChanges ?? 0)")
                DetailRow(title: "Conflicts", value: "\(bridge.sync.conflicts?.count ?? 0)")
                Toggle("Background Sync", isOn: $backgroundSync)
                Toggle("Automatic Retry", isOn: $retryAutomatically)
            }

            Section("Offline Queue") {
                if bridge.sync.offlineQueue?.isEmpty != false {
                    Text("No queued offline changes.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(bridge.sync.offlineQueue ?? [], id: \.self) { item in
                        Text(item)
                    }
                }
            }

            Section("Merge Status") {
                if bridge.sync.conflicts?.isEmpty != false {
                    Label("No conflicts", systemImage: "checkmark.circle")
                        .foregroundStyle(.green)
                } else {
                    ForEach(bridge.sync.conflicts ?? [], id: \.self) { conflict in
                        Text(conflict)
                            .foregroundStyle(.orange)
                    }
                }
            }

            Section("Actions") {
                Button("Sync Now") {
                    syncMessage = "Manual sync queued for Rust sync bridge."
                }
                Button("Retry Pending Uploads") {
                    syncMessage = "Retry requested."
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
            await bridge.refreshSync()
        }
    }
}
