import BackgroundTasks
import SwiftData
import SwiftUI

@main
struct KE8YGWLoggerApp: App {
    private let backgroundRetryCoordinator = SyncBackgroundRetryCoordinator.shared
    private let modelContainer: ModelContainer

    init() {
        do {
            modelContainer = try ModelContainer(for: QSO.self, StationProfile.self, StationEquipment.self, AppSettings.self)
        } catch {
            fatalError("Unable to create SwiftData model container: \(error)")
        }
        backgroundRetryCoordinator.registerLaunchHandler(modelContainer: modelContainer)
    }

    var body: some Scene {
        WindowGroup {
            RootView(backgroundRetryCoordinator: backgroundRetryCoordinator)
        }
        .modelContainer(modelContainer)
    }
}

final class SyncBackgroundRetryCoordinator {
    static let shared = SyncBackgroundRetryCoordinator()

    private var registrationAttempted = false
    private let credentialVault: CredentialVault
    private let policy: SyncBackgroundRetryPolicy

    init(
        credentialVault: CredentialVault = KeychainCredentialVault(),
        policy: SyncBackgroundRetryPolicy = SyncBackgroundRetryPolicy()
    ) {
        self.credentialVault = credentialVault
        self.policy = policy
    }

    func registerLaunchHandler(modelContainer: ModelContainer) {
        guard !registrationAttempted else { return }
        registrationAttempted = true

        _ = BGTaskScheduler.shared.register(
            forTaskWithIdentifier: SyncBackgroundRetryTask.identifier,
            using: nil
        ) { task in
            guard let task = task as? BGProcessingTask else {
                task.setTaskCompleted(success: false)
                return
            }
            Task { @MainActor in
                await self.handle(task: task, modelContainer: modelContainer)
            }
        }
    }

    @discardableResult
    func scheduleIfEligible(
        syncSettings: RustSyncSettings?,
        pendingChanges: Int?,
        now: Date = Date()
    ) -> SyncBackgroundRetryScheduleDecision {
        let decision = policy.decision(
            syncSettings: syncSettings,
            pendingChanges: pendingChanges,
            hasSyncToken: hasSyncToken(),
            now: now
        )
        guard decision.shouldSchedule else {
            return decision
        }

        let request = BGProcessingTaskRequest(identifier: SyncBackgroundRetryTask.identifier)
        request.requiresNetworkConnectivity = true
        request.requiresExternalPower = false
        request.earliestBeginDate = decision.earliestBeginDate
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            return decision.replacingSubmissionError(error.localizedDescription)
        }
        return decision
    }

    @MainActor
    private func handle(task: BGProcessingTask, modelContainer: ModelContainer) async {
        let retryTask = Task { @MainActor in
            await executeStoredRetry(modelContainer: modelContainer)
        }
        task.expirationHandler = {
            retryTask.cancel()
        }
        let result = await retryTask.value
        if let syncSettings = result.syncSettings {
            _ = scheduleIfEligible(syncSettings: syncSettings, pendingChanges: result.remainingPendingChanges)
        }
        task.setTaskCompleted(success: result.taskCompleted)
    }

    @MainActor
    private func executeStoredRetry(modelContainer: ModelContainer) async -> SyncBackgroundRetryRunResult {
        let bridge = RustBridgeStore()
        do {
            let settingsResult = try await bridge.loadSettings()
            guard let syncSettings = settingsResult.settings?.sync else {
                return SyncBackgroundRetryRunResult(taskCompleted: false, syncSettings: nil, remainingPendingChanges: nil)
            }
            guard policy.hasValidServerURL(syncSettings.syncServerUrl),
                  let serverURL = URL(string: syncSettings.syncServerUrl)
            else {
                return SyncBackgroundRetryRunResult(
                    taskCompleted: false,
                    syncSettings: syncSettings,
                    remainingPendingChanges: nil
                )
            }
            guard let syncToken = readSyncToken() else {
                return SyncBackgroundRetryRunResult(
                    taskCompleted: true,
                    syncSettings: syncSettings,
                    remainingPendingChanges: nil
                )
            }

            let result = try await bridge.executeBackgroundSync(
                serverURL: serverURL,
                syncToken: syncToken,
                pushEndpointStyle: SyncPushEndpointStyle(setting: syncSettings.syncEndpointStyle),
                pullEndpointStyle: SyncPullEndpointStyle(setting: syncSettings.syncEndpointStyle),
                autoPullEnabled: syncSettings.autoPullEnabled,
                maxMutations: SyncBackgroundRetryTask.maxMutations,
                networkAvailable: true,
                backgroundTimeBudgetSeconds: SyncBackgroundRetryTask.backgroundTimeBudgetSeconds,
                pushTransport: SyncHTTPTransport(),
                pullTransport: SyncHTTPTransport()
            )
            var taskCompleted = !Task.isCancelled && result.taskCompleted
            if result.pullResult != nil {
                do {
                    try await ProjectionRefreshService.refreshQSOProjection(
                        from: bridge,
                        modelContainer: modelContainer
                    )
                } catch {
                    bridge.lastError = error.localizedDescription
                    taskCompleted = false
                }
            }
            return SyncBackgroundRetryRunResult(
                taskCompleted: taskCompleted,
                syncSettings: syncSettings,
                remainingPendingChanges: bridge.sync.pendingChanges
            )
        } catch {
            return SyncBackgroundRetryRunResult(taskCompleted: false, syncSettings: nil, remainingPendingChanges: nil)
        }
    }

    private func hasSyncToken() -> Bool {
        readSyncToken() != nil
    }

    private func readSyncToken() -> String? {
        let token = try? credentialVault
            .read(account: "sync_token", providerId: "sync")?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let token, !token.isEmpty else {
            return nil
        }
        return token
    }
}
