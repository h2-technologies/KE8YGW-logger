import Network
import XCTest
@testable import KE8YGWLogger

final class RustBridgeTests: XCTestCase {
    func testFallbackBridgeVersionEnvelopeDecodes() async throws {
        let client = FallbackRustBridgeClient()
        let data = try await client.call(.version, argument: nil)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(RustBridgeEnvelope<BridgeVersion>.self, from: data)

        XCTAssertTrue(envelope.ok)
        XCTAssertEqual(envelope.data?.app, "KE8YGW Logger")
        XCTAssertEqual(envelope.data?.bridgeVersion, 1)
    }

    func testFallbackProviderSnapshotIncludesRequiredProviders() async throws {
        let client = FallbackRustBridgeClient()
        let data = try await client.call(.providers, argument: nil)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(RustBridgeEnvelope<ProviderStatusSnapshot>.self, from: data)
        let providers = envelope.data?.onlineProviders.map { $0.providerId } ?? []

        XCTAssertTrue(providers.contains("qrz"))
        XCTAssertTrue(providers.contains("lotw"))
        XCTAssertTrue(providers.contains("sotawatch"))
    }

    func testFallbackLookupNormalizesCallsign() async throws {
        let client = FallbackRustBridgeClient()
        let data = try await client.call(.lookupCallsign, argument: " ke8ygw ")
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(RustBridgeEnvelope<CallsignLookupPayload>.self, from: data)

        XCTAssertEqual(envelope.data?.callsign, "KE8YGW")
    }

    func testFallbackCreateQSOMutationDecodes() async throws {
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let result = try await store.createQSO(CreateQSOBridgeRequest(
            appSupportDir: "/tmp/ke8ygw-ios-test",
            operationId: "op-1",
            deviceId: nil,
            qso: CreateQSOBridgePayload(
                contactedCallsign: "K1ABC",
                stationCallsign: "KE8YGW",
                operatorCallsign: "KE8YGW",
                startedAt: "2026-07-10T12:00:00Z",
                mode: "SSB",
                band: "20m"
            )
        ))

        XCTAssertTrue(result.accepted)
        XCTAssertEqual(result.officialEvent.eventType, "official.log.qso.created")
        XCTAssertEqual(result.qso?.payload.contactedCallsign, "K1ABC")
    }

    func testFallbackBridgeSelfTestDecodes() async throws {
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let result = try await store.bridgeSelfTest()

        XCTAssertTrue(result.success)
        XCTAssertEqual(result.abiVersion, 1)
        XCTAssertEqual(result.bridgeSchemaVersion, 1)
    }

    func testFallbackSyncRetryPlanBlocksWithoutNetwork() async throws {
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let result = try await store.planOfflineRetry(
            maxMutations: 3,
            markSending: true,
            networkAvailable: false,
            backgroundTimeBudgetSeconds: 12
        )

        XCTAssertTrue(result.retryPlan.networkRequired)
        XCTAssertTrue(result.retryPlan.blockedByNetwork)
        XCTAssertFalse(result.retryPlan.markSending)
        XCTAssertEqual(result.retryPlan.maxMutations, 3)
        XCTAssertEqual(result.retryPlan.backgroundTimeBudgetSeconds, 12)
        XCTAssertEqual(result.retryPlan.operationIds.count, 0)
        XCTAssertEqual(result.retryPlan.transportableEvents.count, 0)
        XCTAssertEqual(result.offlineQueue.health.total, 0)
    }

    func testFallbackSyncSnapshotDecodesLocalIdentity() async throws {
        let client = FallbackRustBridgeClient()
        let snapshot: SyncSnapshot = try await decodeCommand(
            client: client,
            command: "sync.snapshot",
            payload: ["app_support_dir": "swift-sync-\(UUID().uuidString)"]
        )

        XCTAssertEqual(snapshot.identity?.deviceId, "00000000-0000-4000-8000-0000000000f1")
        XCTAssertEqual(snapshot.identity?.sessionId, "00000000-0000-4000-8000-0000000000f2")
        XCTAssertEqual(snapshot.identity?.displayName, "KE8YGW Logger iOS")
        XCTAssertEqual(snapshot.identity?.capabilities.contains("handshake.v1"), true)
        XCTAssertNil(snapshot.identity?.localApiPort)
    }

    func testFallbackSyncRetryResultSurfacesUserActionFailures() async throws {
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let operationID = "11111111-1111-4111-8111-111111111111"
        let result = try await store.recordOfflineRetryResult(
            operationIds: [operationID],
            result: .authFailed
        )

        XCTAssertEqual(result.retryResult.result, .authFailed)
        XCTAssertEqual(result.retryResult.operationIds, [operationID])
        XCTAssertEqual(result.affectedMutations.first?.status, "user_action_required")
        XCTAssertEqual(result.affectedMutations.first?.lastErrorCode, "auth_failed")
        XCTAssertEqual(result.offlineQueue.health.userActionRequired, 1)
    }

    func testFallbackRemoteEventsApplyDecodesPullResponse() async throws {
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let event = SyncOfficialEvent(
            eventId: "11111111-1111-4111-8111-111111111111",
            eventType: "official.log.qso.created",
            logbookId: logbookID,
            entityId: "22222222-2222-4222-8222-222222222222",
            previousHash: nil,
            eventHash: "remote-head-hash",
            timestamp: "2026-07-10T12:00:00Z",
            authorOperatorId: nil,
            stationCallsign: "KE8YGW",
            operatorCallsign: "KE8YGW",
            authorDeviceId: "33333333-3333-4333-8333-333333333333",
            sourceDeviceId: "33333333-3333-4333-8333-333333333333",
            correlationId: "44444444-4444-4444-8444-444444444444",
            sourcePluginId: "plugin.ios.native",
            schemaVersion: 1,
            payload: .object(["contacted_callsign": .string("K1ABC")])
        )

        let result = try await store.applyRemoteEvents(
            logbookId: logbookID,
            peerId: "ios-test-peer",
            events: [event]
        )

        XCTAssertEqual(result.pull.peerId, "ios-test-peer")
        XCTAssertEqual(result.pull.status, "pulled")
        XCTAssertEqual(result.pull.acceptedCount, 1)
        XCTAssertEqual(result.pull.remoteHeadHash, "remote-head-hash")
        XCTAssertEqual(result.projection?.pendingEventCount, 1)
    }

    func testFallbackConflictReviewSnapshotDecodesOpenReview() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-conflict-\(UUID().uuidString)"
        let result: SyncConflictReviewMutationResult = try await decodeCommand(
            client: client,
            command: "sync.conflict_reviews.create",
            payload: [
                "app_support_dir": appSupportDir,
                "report": conflictReportPayload()
            ]
        )

        XCTAssertEqual(result.conflictReviews.health?.open, 1)
        XCTAssertEqual(result.conflictReviews.openReviews.count, 1)
        XCTAssertEqual(result.conflictReview.status, "open")
        XCTAssertEqual(result.conflictReview.report?.status, "diverged")
        XCTAssertEqual(result.conflictReview.report?.recommendedAction, "Manual review required before syncing.")
        XCTAssertEqual(result.conflictReview.report?.conflicts?.first?.kind, "divergent_heads")
        XCTAssertEqual(result.conflictReview.report?.conflicts?.first?.requiresUserAction, true)
    }

    func testFallbackConflictReviewResolutionDecodesSelectedRecoveryPath() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-conflict-\(UUID().uuidString)"
        let created: SyncConflictReviewMutationResult = try await decodeCommand(
            client: client,
            command: "sync.conflict_reviews.create",
            payload: [
                "app_support_dir": appSupportDir,
                "report": conflictReportPayload()
            ]
        )
        let reviewID = try XCTUnwrap(created.conflictReview.reviewId)
        let resolved: SyncConflictReviewMutationResult = try await decodeCommand(
            client: client,
            command: "sync.conflict_reviews.resolve",
            payload: [
                "app_support_dir": appSupportDir,
                "review_id": reviewID,
                "resolution": [
                    "choice": SyncManualConflictResolutionChoice.markUserActionRequired.rawValue,
                    "operator_note": "Reviewed on iOS.",
                    "corrective_event_hashes": []
                ]
            ]
        )

        XCTAssertEqual(resolved.conflictReview.status, "resolved")
        XCTAssertEqual(resolved.conflictReview.selectedResolution?.choice, .markUserActionRequired)
        XCTAssertEqual(resolved.conflictReview.selectedResolution?.operatorNote, "Reviewed on iOS.")
        XCTAssertEqual(resolved.conflictReviews.health?.open, 0)
        XCTAssertEqual(resolved.conflictReviews.health?.resolved, 1)
    }

    func testFallbackLanTrustSnapshotDecodesCredentialReferences() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-lan-\(UUID().uuidString)"
        let peerDeviceID = UUID().uuidString
        let authCredentialID = UUID().uuidString
        let trusted: SyncLanTrustedDeviceBridgeResult = try await decodeCommand(
            client: client,
            command: "sync.lan_trust.trust_peer",
            payload: [
                "app_support_dir": appSupportDir,
                "peer_device_id": peerDeviceID,
                "peer_display_name": "Desktop LAN Peer",
                "auth_credential_id": authCredentialID,
                "public_key_fingerprint": "sha256:test"
            ]
        )
        let snapshot: SyncSnapshot = try await decodeCommand(
            client: client,
            command: "sync.snapshot",
            payload: ["app_support_dir": appSupportDir]
        )

        XCTAssertEqual(trusted.trustedDevice.deviceId, peerDeviceID)
        XCTAssertEqual(snapshot.lanTrust?.trustedDevices.first?.authCredentialId, authCredentialID)
        XCTAssertEqual(snapshot.lanTrust?.trustedDevices.first?.publicKeyFingerprint, "sha256:test")
        XCTAssertNil(snapshot.lanTrustError)
    }

    func testFallbackLanPairingTokenDoesNotPersistPairingCodeInSnapshot() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-lan-\(UUID().uuidString)"
        let issued: SyncLanPairingTokenBridgeResult = try await decodeCommand(
            client: client,
            command: "sync.lan_trust.issue_pairing_token",
            payload: [
                "app_support_dir": appSupportDir,
                "issuer_display_name": "iOS Field Phone",
                "approved_by_operator": true
            ]
        )
        let request = try JSONSerialization.data(withJSONObject: [
            "command": "sync.lan_trust.snapshot",
            "correlation_id": UUID().uuidString,
            "payload": ["app_support_dir": appSupportDir]
        ])
        let snapshotData = try await client.callJSON(request)
        let snapshotJSON = String(decoding: snapshotData, as: UTF8.self)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(RustBridgeEnvelope<SyncLanTrustBridgeResult>.self, from: snapshotData)
        let snapshot = try XCTUnwrap(envelope.data)

        XCTAssertFalse(issued.pairing.pairingCode.isEmpty)
        XCTAssertEqual(snapshot.lanTrust.pairingTokens.first?.tokenId, issued.pairing.tokenId)
        XCTAssertEqual(snapshot.lanTrust.pairingTokens.first?.issuerDeviceId, "00000000-0000-4000-8000-0000000000f1")
        XCTAssertFalse(snapshotJSON.contains(issued.pairing.pairingCode))
    }

    @MainActor
    func testLanTrustBridgeMethodsUpdateSyncSnapshot() async throws {
        let store = RustBridgeStore(client: FallbackRustBridgeClient())
        let peerDeviceID = UUID().uuidString
        let authCredentialID = UUID().uuidString
        let rotatedCredentialID = UUID().uuidString
        let trusted = try await store.trustLanPeer(
            peerDeviceId: peerDeviceID,
            peerDisplayName: "Desktop LAN Peer",
            authCredentialId: authCredentialID
        )
        let rotated = try await store.rotateLanAuthCredential(
            deviceId: peerDeviceID,
            newAuthCredentialId: rotatedCredentialID
        )
        let revoked = try await store.revokeLanPeer(deviceId: peerDeviceID)

        XCTAssertEqual(trusted.trustedDevice.authCredentialId, authCredentialID)
        XCTAssertEqual(rotated.rotation.previousAuthCredentialId, authCredentialID)
        XCTAssertEqual(rotated.rotation.trustedDevice.authCredentialId, rotatedCredentialID)
        XCTAssertEqual(revoked.trustedDevice.deviceId, peerDeviceID)
        XCTAssertNotNil(store.sync.lanTrust?.trustedDevices.first(where: { $0.deviceId == peerDeviceID })?.revokedAt)
    }

    @MainActor
    func testLanPairingAcceptBridgeConsumesTokenAndStoresCredentialReference() async throws {
        let store = RustBridgeStore(client: FallbackRustBridgeClient())
        let peerDeviceID = UUID().uuidString
        let authCredentialID = UUID().uuidString
        let issued = try await store.issueLanPairingToken(
            issuerDisplayName: "iOS Field Phone",
            approvedByOperator: true
        )

        let accepted = try await store.acceptLanPairingToken(
            tokenId: issued.pairing.tokenId,
            pairingCode: issued.pairing.pairingCode,
            peerDeviceId: peerDeviceID,
            peerDisplayName: "Desktop LAN Peer",
            publicKeyFingerprint: "sha256:desktop",
            authCredentialId: authCredentialID
        )

        XCTAssertEqual(accepted.trustedDevice.deviceId, peerDeviceID)
        XCTAssertEqual(accepted.trustedDevice.authCredentialId, authCredentialID)
        XCTAssertEqual(accepted.trustedDevice.pairingTokenId, issued.pairing.tokenId)
        XCTAssertEqual(accepted.trustedDevice.publicKeyFingerprint, "sha256:desktop")
        XCTAssertNotNil(store.sync.lanTrust?.pairingTokens.first(where: { $0.tokenId == issued.pairing.tokenId })?.consumedAt)
    }

    func testSyncRetryPlanDecodesTransportableOfficialEvents() throws {
        let event = officialEventPayload()
        let envelope: [String: Any] = [
            "ok": true,
            "bridge_version": 1,
            "abi_version": 1,
            "schema_version": 1,
            "generated_at": "2026-07-21T12:00:00Z",
            "correlation_id": UUID().uuidString,
            "error": NSNull(),
            "data": [
                "retry_plan": [
                    "schema_version": 1,
                    "logbook_id": event["logbook_id"] ?? "",
                    "operation_ids": [UUID().uuidString],
                    "event_hashes": [event["event_hash"] ?? ""],
                    "events": [event],
                    "missing_local_event_operation_ids": [],
                    "network_required": true,
                    "blocked_by_network": false,
                    "max_mutations": 1,
                    "background_time_budget_seconds": 20,
                    "mark_sending": true,
                    "permanent_failure_results": ["auth_failed", "diverged"]
                ],
                "offline_queue": FallbackBridgeData.offlineQueue(),
                "recovery": FallbackBridgeData.offlineQueueRecovery()
            ]
        ]
        let data = try JSONSerialization.data(withJSONObject: envelope)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let decoded = try decoder.decode(RustBridgeEnvelope<SyncRetryPlanBridgeResult>.self, from: data)
        let plan = try XCTUnwrap(decoded.data?.retryPlan)
        let decodedEvent = try XCTUnwrap(plan.transportableEvents.first)

        XCTAssertEqual(decodedEvent.eventHash, event["event_hash"] as? String)
        XCTAssertEqual(decodedEvent.eventType, "official.log.qso.created")
        XCTAssertEqual(plan.eventHashes, [decodedEvent.eventHash])
        guard case .object(let payload) = decodedEvent.payload else {
            return XCTFail("Expected object payload")
        }
        XCTAssertEqual(payload["contacted_callsign"], .string("K1ABC"))
        XCTAssertEqual(payload["portable"], .bool(true))
    }

    func testSyncHTTPTransportBuildsHostedPushRequestFromPlannedEvents() throws {
        let event = sampleOfficialEvent()
        let request = try SyncHTTPTransport().makePushRequest(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test/root/")),
            bearerToken: "secret-bearer",
            syncToken: nil,
            logbookId: event.logbookId,
            events: [event]
        )
        let body = try XCTUnwrap(request.httpBody)
        let bodyString = String(decoding: body, as: UTF8.self)
        let object = try JSONSerialization.jsonObject(with: body) as? [String: Any]
        let events = object?["events"] as? [[String: Any]]

        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(
            request.url?.absoluteString,
            "https://sync.example.test/root/api/v1/logbooks/\(event.logbookId)/push"
        )
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer secret-bearer")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(object?["logbook_id"] as? String, event.logbookId)
        XCTAssertEqual(events?.first?["event_hash"] as? String, event.eventHash)
        XCTAssertFalse(bodyString.contains("secret-bearer"))
    }

    func testSyncHTTPTransportBuildsHostedSyncPushRequestFromPlannedEvents() throws {
        let event = sampleOfficialEvent()
        let request = try SyncHTTPTransport().makePushRequest(
            serverURL: try XCTUnwrap(URL(string: "https://api.example.test/root/")),
            bearerToken: "secret-bearer",
            syncToken: "sync-secret",
            endpointStyle: .hostedSync,
            logbookId: event.logbookId,
            events: [event]
        )
        let body = try XCTUnwrap(request.httpBody)
        let bodyString = String(decoding: body, as: UTF8.self)
        let object = try JSONSerialization.jsonObject(with: body) as? [String: Any]
        let auth = object?["auth"] as? [String: Any]

        XCTAssertEqual(request.url?.absoluteString, "https://api.example.test/root/api/v1/sync/push")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer secret-bearer")
        XCTAssertEqual(object?["logbook_id"] as? String, event.logbookId)
        XCTAssertEqual(auth?["sync_token"] as? String, "sync-secret")
        XCTAssertFalse(bodyString.contains("secret-bearer"))
    }

    func testSyncHTTPTransportBuildsLogbookScopedPullRequest() throws {
        let logbookID = UUID().uuidString
        let request = try SyncHTTPTransport().makePullRequest(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test/root/")),
            bearerToken: nil,
            syncToken: "sync-secret",
            logbookId: logbookID,
            localHeadHash: "local-head"
        )
        let body = try XCTUnwrap(request.httpBody)
        let bodyString = String(decoding: body, as: UTF8.self)
        let object = try JSONSerialization.jsonObject(with: body) as? [String: Any]
        let auth = object?["auth"] as? [String: Any]

        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(
            request.url?.absoluteString,
            "https://sync.example.test/root/api/v1/logbooks/\(logbookID)/pull"
        )
        XCTAssertEqual(auth?["sync_token"] as? String, "sync-secret")
        XCTAssertEqual(object?["logbook_id"] as? String, logbookID)
        XCTAssertEqual(object?["local_head_hash"] as? String, "local-head")
        XCTAssertFalse(bodyString.contains("Bearer"))
    }

    func testSyncHTTPTransportBuildsHostedSyncPullRequest() throws {
        let logbookID = UUID().uuidString
        let request = try SyncHTTPTransport().makePullRequest(
            serverURL: try XCTUnwrap(URL(string: "https://api.example.test/root/")),
            bearerToken: "secret-bearer",
            syncToken: nil,
            endpointStyle: .hostedSync,
            logbookId: logbookID,
            localHeadHash: nil
        )
        let body = try XCTUnwrap(request.httpBody)
        let bodyString = String(decoding: body, as: UTF8.self)
        let object = try JSONSerialization.jsonObject(with: body) as? [String: Any]

        XCTAssertEqual(request.url?.absoluteString, "https://api.example.test/root/api/v1/sync/pull")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer secret-bearer")
        XCTAssertEqual(object?["logbook_id"] as? String, logbookID)
        XCTAssertNil(object?["auth"])
        XCTAssertFalse(bodyString.contains("secret-bearer"))
    }

    func testSyncEndpointStyleSettingsMapToNativeTransportPaths() throws {
        XCTAssertEqual(SyncPushEndpointStyle(setting: nil), .logbookScoped)
        XCTAssertEqual(SyncPullEndpointStyle(setting: "logbook_scoped"), .logbookScoped)
        XCTAssertEqual(SyncPushEndpointStyle(setting: "hosted_sync"), .hostedSync)
        XCTAssertEqual(SyncPullEndpointStyle(setting: "hosted"), .hostedSync)
        XCTAssertEqual(SyncPushEndpointStyle(setting: "unsupported"), .logbookScoped)
    }

    func testSyncLanHTTPTransportBuildsSignedEventsSinceRequest() throws {
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let localDeviceID = "00000000-0000-4000-8000-0000000000f1"
        let request = try SyncLanHTTPTransport().makeEventsSinceRequest(
            peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673")),
            localDeviceId: localDeviceID.uppercased(),
            logbookId: logbookID.uppercased(),
            localHeadHash: "local-head",
            authSecret: "lan-secret",
            replayNonce: "nonce-fixed"
        )

        XCTAssertEqual(request.httpMethod, "GET")
        XCTAssertEqual(
            request.url?.absoluteString,
            "http://192.168.1.20:17673/api/sync/events-since?logbook_id=\(logbookID)&after_hash=local-head"
        )
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.value(forHTTPHeaderField: "x-ke8ygw-lan-device-id"), localDeviceID)
        XCTAssertEqual(request.value(forHTTPHeaderField: "x-ke8ygw-lan-replay-nonce"), "nonce-fixed")
        XCTAssertEqual(request.value(forHTTPHeaderField: "x-ke8ygw-lan-signature-version"), "hmac-sha256-v1")
        XCTAssertEqual(
            request.value(forHTTPHeaderField: "x-ke8ygw-lan-signature"),
            "d8539f894553c9b5dd6804d40d4ddc3a1c8545ce59d5c2cb14027cbc15df3f15"
        )
    }

    func testSyncLanHTTPTransportBuildsUnsignedStateRequest() throws {
        let request = try SyncLanHTTPTransport().makeStateRequest(
            peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673"))
        )

        XCTAssertEqual(request.httpMethod, "GET")
        XCTAssertEqual(request.url?.absoluteString, "http://192.168.1.20:17673/api/sync/state")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertNil(request.value(forHTTPHeaderField: "x-ke8ygw-lan-device-id"))
        XCTAssertNil(request.value(forHTTPHeaderField: "x-ke8ygw-lan-replay-nonce"))
        XCTAssertNil(request.value(forHTTPHeaderField: "x-ke8ygw-lan-signature-version"))
        XCTAssertNil(request.value(forHTTPHeaderField: "x-ke8ygw-lan-signature"))
    }

    func testSyncLanHTTPTransportAcceptsMatchingPeerIdentity() throws {
        let peerDeviceID = "00000000-0000-4000-8000-0000000000f3"
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: peerDeviceID.uppercased(),
            displayName: "Desktop LAN Peer",
            logbookIds: nil,
            trustedAt: "2026-07-21T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )
        let state = SyncLanPeerStateResponse(
            identity: SyncPeerIdentity(
                deviceId: peerDeviceID,
                sessionId: "00000000-0000-4000-8000-0000000000aa",
                userHash: nil,
                displayName: "Desktop LAN Peer",
                capabilities: ["discovery.v1", "handshake.v1"],
                localApiPort: 17673
            ),
            localHead: nil
        )

        XCTAssertNoThrow(
            try SyncLanHTTPTransport().validatePeerState(state, trustedDevice: trustedDevice)
        )
    }

    func testSyncLanHTTPTransportRejectsMismatchedPeerIdentity() throws {
        let expectedDeviceID = "00000000-0000-4000-8000-0000000000f3"
        let actualDeviceID = "00000000-0000-4000-8000-0000000000f4"
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: expectedDeviceID,
            displayName: "Desktop LAN Peer",
            logbookIds: nil,
            trustedAt: "2026-07-21T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )
        let state = SyncLanPeerStateResponse(
            identity: SyncPeerIdentity(
                deviceId: actualDeviceID,
                sessionId: "00000000-0000-4000-8000-0000000000aa",
                userHash: nil,
                displayName: "Wrong LAN Peer",
                capabilities: ["discovery.v1", "handshake.v1"],
                localApiPort: 17673
            ),
            localHead: nil
        )

        XCTAssertThrowsError(
            try SyncLanHTTPTransport().validatePeerState(state, trustedDevice: trustedDevice)
        ) { error in
            XCTAssertEqual(
                error as? SyncLanHTTPTransportError,
                .peerIdentityMismatch(expectedDeviceId: expectedDeviceID, actualDeviceId: actualDeviceID)
            )
        }
    }

    func testSyncLanHTTPTransportRejectsMissingPeerIdentity() throws {
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: "00000000-0000-4000-8000-0000000000f3",
            displayName: "Desktop LAN Peer",
            logbookIds: nil,
            trustedAt: "2026-07-21T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )

        XCTAssertThrowsError(
            try SyncLanHTTPTransport().validatePeerState(
                SyncLanPeerStateResponse(identity: nil, localHead: nil),
                trustedDevice: trustedDevice
            )
        ) { error in
            XCTAssertEqual(error as? SyncLanHTTPTransportError, .missingPeerIdentity)
        }
    }

    func testSyncLanHTTPPairingTransportBuildsRemoteAcceptRequest() throws {
        let tokenID = "00000000-0000-4000-8000-0000000000c1"
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let localIdentity = SyncPeerIdentity(
            deviceId: "00000000-0000-4000-8000-0000000000f1",
            sessionId: "00000000-0000-4000-8000-0000000000f2",
            userHash: nil,
            displayName: "iOS Field Phone",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: nil
        )
        let request = try SyncLanHTTPPairingTransport().makePairingAcceptRequest(
            peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673")),
            tokenId: tokenID.uppercased(),
            pairingCode: "pairing-code",
            authSecret: "lan-auth-secret-00000000000000000000",
            localIdentity: localIdentity,
            logbookId: logbookID.uppercased(),
            publicKeyFingerprint: "sha256:peer"
        )
        let body = try XCTUnwrap(request.httpBody)
        let object = try JSONSerialization.jsonObject(with: body) as? [String: Any]

        XCTAssertEqual(request.httpMethod, "POST")
        XCTAssertEqual(request.url?.absoluteString, "http://192.168.1.20:17673/api/sync/lan/pairing-accept")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertNil(request.value(forHTTPHeaderField: "x-ke8ygw-lan-signature"))
        XCTAssertEqual(object?["token_id"] as? String, tokenID)
        XCTAssertEqual(object?["pairing_code"] as? String, "pairing-code")
        XCTAssertEqual(object?["auth_code"] as? String, "lan-auth-secret-00000000000000000000")
        XCTAssertEqual(object?["peer_device_id"] as? String, localIdentity.deviceId)
        XCTAssertEqual(object?["peer_display_name"] as? String, "iOS Field Phone")
        XCTAssertEqual(object?["logbook_id"] as? String, logbookID)
        XCTAssertEqual(object?["public_key_fingerprint"] as? String, "sha256:peer")
    }

    func testSyncLanDiscoveryDecodesPacketAndBuildsPeerURL() throws {
        let json = """
        {
          "protocol_name": "ke8ygw-logger-sync",
          "protocol_version": 1,
          "device_id": "00000000-0000-4000-8000-0000000000f3",
          "session_id": "00000000-0000-4000-8000-0000000000f4",
          "user_hash": null,
          "display_name": "Desktop LAN Peer",
          "capabilities": ["discovery.v1", "handshake.v1"],
          "local_api_port": 17673,
          "timestamp": "2026-07-22T12:00:00Z"
        }
        """
        let packet = try XCTUnwrap(SyncLanDiscoveryScanner.decodeDiscoveryPacket(Data(json.utf8)))
        let endpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host("192.168.1.20"),
            port: try XCTUnwrap(NWEndpoint.Port(rawValue: 50300))
        )
        let peerURL = try XCTUnwrap(SyncLanDiscoveryScanner.peerURL(packet: packet, remoteEndpoint: endpoint))

        XCTAssertTrue(SyncLanDiscoveryScanner.isSupported(packet))
        XCTAssertEqual(peerURL.absoluteString, "http://192.168.1.20:17673")
    }

    func testSyncLanDiscoveryRejectsSelfAndUnscopedLinkLocalIPv6() throws {
        let identity = SyncPeerIdentity(
            deviceId: "00000000-0000-4000-8000-0000000000f1",
            sessionId: "00000000-0000-4000-8000-0000000000f2",
            userHash: nil,
            displayName: "iOS Field Phone",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: nil
        )
        let selfPacket = SyncLanDiscoveryPacket(
            protocolName: "ke8ygw-logger-sync",
            protocolVersion: 1,
            deviceId: identity.deviceId,
            sessionId: identity.sessionId,
            userHash: nil,
            displayName: identity.displayName,
            capabilities: identity.capabilities,
            localApiPort: 17673,
            timestamp: "2026-07-22T12:00:00Z"
        )
        let peerPacket = SyncLanDiscoveryPacket(
            protocolName: "ke8ygw-logger-sync",
            protocolVersion: 1,
            deviceId: "00000000-0000-4000-8000-0000000000f3",
            sessionId: "00000000-0000-4000-8000-0000000000f4",
            userHash: nil,
            displayName: "Desktop LAN Peer",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: 17673,
            timestamp: "2026-07-22T12:00:00Z"
        )
        let linkLocalEndpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host("fe80::1234"),
            port: try XCTUnwrap(NWEndpoint.Port(rawValue: 50300))
        )

        XCTAssertTrue(SyncLanDiscoveryScanner.isSelf(selfPacket, identity: identity))
        XCTAssertNil(SyncLanDiscoveryScanner.peerURL(packet: peerPacket, remoteEndpoint: linkLocalEndpoint))
    }

    func testSyncLanDiscoveryRequiresProbedIdentityToMatchPacket() {
        let packet = SyncLanDiscoveryPacket(
            protocolName: "ke8ygw-logger-sync",
            protocolVersion: 1,
            deviceId: "00000000-0000-4000-8000-0000000000f3",
            sessionId: "00000000-0000-4000-8000-0000000000f4",
            userHash: nil,
            displayName: "Desktop LAN Peer",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: 17673,
            timestamp: "2026-07-22T12:00:00Z"
        )
        let matchingState = SyncLanPeerStateResponse(
            identity: SyncPeerIdentity(
                deviceId: packet.deviceId,
                sessionId: packet.sessionId,
                userHash: nil,
                displayName: packet.displayName,
                capabilities: packet.capabilities,
                localApiPort: Int(packet.localApiPort ?? 0)
            ),
            localHead: nil
        )
        let spoofedState = SyncLanPeerStateResponse(
            identity: SyncPeerIdentity(
                deviceId: packet.deviceId,
                sessionId: "00000000-0000-4000-8000-0000000000ff",
                userHash: nil,
                displayName: packet.displayName,
                capabilities: packet.capabilities,
                localApiPort: Int(packet.localApiPort ?? 0)
            ),
            localHead: nil
        )

        XCTAssertTrue(SyncLanDiscoveryScanner.peerStateMatches(packet: packet, state: matchingState))
        XCTAssertFalse(SyncLanDiscoveryScanner.peerStateMatches(packet: packet, state: spoofedState))
    }

    func testSyncLanHTTPPairingTransportValidatesRemoteAcceptResponse() throws {
        let localIdentity = SyncPeerIdentity(
            deviceId: "00000000-0000-4000-8000-0000000000f1",
            sessionId: "00000000-0000-4000-8000-0000000000f2",
            userHash: nil,
            displayName: "iOS Field Phone",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: nil
        )
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: localIdentity.deviceId,
            displayName: localIdentity.displayName,
            logbookIds: [logbookID],
            trustedAt: "2026-07-22T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )

        XCTAssertNoThrow(
            try SyncLanHTTPPairingTransport().validateRemotePairingResponse(
                SyncLanPairingAcceptResponse(ok: true, trustedDevice: trustedDevice),
                localIdentity: localIdentity,
                logbookId: logbookID
            )
        )
    }

    func testSyncLanHTTPPairingTransportRejectsWrongRemoteAcceptDevice() throws {
        let localIdentity = SyncPeerIdentity(
            deviceId: "00000000-0000-4000-8000-0000000000f1",
            sessionId: "00000000-0000-4000-8000-0000000000f2",
            userHash: nil,
            displayName: "iOS Field Phone",
            capabilities: ["discovery.v1", "handshake.v1"],
            localApiPort: nil
        )
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: "00000000-0000-4000-8000-0000000000ff",
            displayName: "Unexpected Device",
            logbookIds: ["00000000-0000-4000-8000-000000000001"],
            trustedAt: "2026-07-22T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )

        XCTAssertThrowsError(
            try SyncLanHTTPPairingTransport().validateRemotePairingResponse(
                SyncLanPairingAcceptResponse(ok: true, trustedDevice: trustedDevice),
                localIdentity: localIdentity,
                logbookId: "00000000-0000-4000-8000-000000000001"
            )
        ) { error in
            XCTAssertEqual(error as? SyncLanHTTPTransportError, .invalidPeerPayload)
        }
    }

    @MainActor
    func testLanPairingExecutorCompletesRemoteAcceptAndStoresLocalTrust() async throws {
        let tokenID = "00000000-0000-4000-8000-0000000000c1"
        let peerDeviceID = "00000000-0000-4000-8000-0000000000f3"
        let authCredentialID = UUID().uuidString
        let transport = StubSyncLanPairingTransport(
            peerIdentity: SyncPeerIdentity(
                deviceId: peerDeviceID,
                sessionId: "00000000-0000-4000-8000-0000000000f4",
                userHash: nil,
                displayName: "Desktop LAN Peer",
                capabilities: ["discovery.v1", "handshake.v1"],
                localApiPort: 17673
            ),
            remoteTrustedDevice: SyncTrustedPeerDevice(
                deviceId: "00000000-0000-4000-8000-0000000000f1",
                displayName: "KE8YGW Logger iOS",
                logbookIds: ["00000000-0000-4000-8000-000000000001"],
                trustedAt: "2026-07-22T12:00:00Z",
                revokedAt: nil,
                pairingTokenId: tokenID,
                publicKeyFingerprint: "sha256:ios",
                authCredentialId: "remote-credential-id",
                authRotatedAt: nil,
                lastSeenAt: nil
            )
        )
        let store = RustBridgeStore(client: FallbackRustBridgeClient())

        let result = try await store.completeLanPairing(
            peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673")),
            tokenId: tokenID,
            pairingCode: "peer-pairing-code",
            authSecret: "lan-auth-secret-00000000000000000000",
            authCredentialId: authCredentialID,
            publicKeyFingerprint: "sha256:peer",
            transport: transport
        )

        XCTAssertEqual(result.trustedDevice.deviceId, peerDeviceID)
        XCTAssertEqual(result.trustedDevice.displayName, "Desktop LAN Peer")
        XCTAssertEqual(result.trustedDevice.pairingTokenId, tokenID)
        XCTAssertEqual(result.trustedDevice.publicKeyFingerprint, "sha256:peer")
        XCTAssertEqual(result.trustedDevice.authCredentialId, authCredentialID)
        XCTAssertNotNil(store.sync.lanTrust?.trustedDevices.first(where: { $0.deviceId == peerDeviceID }))
        XCTAssertEqual(transport.requests.first?.localIdentity.deviceId, "00000000-0000-4000-8000-0000000000f1")
        XCTAssertEqual(transport.requests.first?.authSecret, "lan-auth-secret-00000000000000000000")
    }

    func testSyncHTTPTransportRejectsEmptyEventBatches() throws {
        XCTAssertThrowsError(
            try SyncHTTPTransport().makePushRequest(
                serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
                bearerToken: nil,
                syncToken: nil,
                logbookId: UUID().uuidString,
                events: []
            )
        ) { error in
            XCTAssertEqual(error as? SyncHTTPTransportError, .emptyEventBatch)
        }
    }

    func testSyncPullExecutorFetchesAndAppliesRemoteEvents() async throws {
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let event = sampleOfficialEvent(logbookID: logbookID, eventHash: "remote-head")
        let transport = StubSyncPullTransport(response: pullResponse(logbookID: logbookID, events: [event]))
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())

        let result = try await store.executeRemotePull(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            transport: transport
        )

        XCTAssertEqual(result.status, .applied)
        XCTAssertEqual(result.acceptedCount, 1)
        XCTAssertEqual(result.applyResult?.pull.remoteHeadHash, "remote-head")
        XCTAssertEqual(transport.requests.first?.syncToken, "sync-secret")
        XCTAssertEqual(transport.requests.first?.logbookId, logbookID)
    }

    func testSyncPullExecutorBlocksWithoutNetwork() async throws {
        let transport = StubSyncPullTransport(response: pullResponse(logbookID: UUID().uuidString, events: []))
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())

        let result = try await store.executeRemotePull(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            networkAvailable: false,
            transport: transport
        )

        XCTAssertEqual(result.status, .blockedByNetwork)
        XCTAssertTrue(transport.requests.isEmpty)
    }

    func testSyncLanPullExecutorFetchesAndAppliesRemoteEvents() async throws {
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let event = sampleOfficialEvent(logbookID: logbookID, eventHash: "lan-remote-head")
        let peerDeviceID = "00000000-0000-4000-8000-0000000000f3"
        let transport = StubSyncLanPullTransport(response: pullResponse(logbookID: logbookID, events: [event]))
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: peerDeviceID,
            displayName: "Desktop LAN Peer",
            logbookIds: [logbookID],
            trustedAt: "2026-07-21T12:00:00Z",
            revokedAt: nil,
            pairingTokenId: nil,
            publicKeyFingerprint: "sha256:desktop",
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )

        let result = try await store.executeLanPull(
            peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673")),
            trustedDevice: trustedDevice,
            authSecret: "lan-secret",
            transport: transport
        )

        XCTAssertEqual(result.status, .applied)
        XCTAssertEqual(result.acceptedCount, 1)
        XCTAssertEqual(result.applyResult?.pull.peerId, peerDeviceID)
        XCTAssertEqual(result.applyResult?.pull.remoteHeadHash, "lan-remote-head")
        XCTAssertEqual(transport.requests.first?.localIdentity.deviceId, "00000000-0000-4000-8000-0000000000f1")
        XCTAssertEqual(transport.requests.first?.trustedDevice.deviceId, peerDeviceID)
        XCTAssertEqual(transport.requests.first?.authSecret, "lan-secret")
    }

    func testSyncLanPullExecutorRejectsRevokedPeerBeforeTransport() async throws {
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let peerDeviceID = "00000000-0000-4000-8000-0000000000f3"
        let transport = StubSyncLanPullTransport(response: pullResponse(logbookID: logbookID, events: []))
        let store = await RustBridgeStore(client: FallbackRustBridgeClient())
        let trustedDevice = SyncTrustedPeerDevice(
            deviceId: peerDeviceID,
            displayName: "Desktop LAN Peer",
            logbookIds: [logbookID],
            trustedAt: "2026-07-21T12:00:00Z",
            revokedAt: "2026-07-21T12:05:00Z",
            pairingTokenId: nil,
            publicKeyFingerprint: nil,
            authCredentialId: "credential-id",
            authRotatedAt: nil,
            lastSeenAt: nil
        )

        do {
            _ = try await store.executeLanPull(
                peerURL: try XCTUnwrap(URL(string: "http://192.168.1.20:17673")),
                trustedDevice: trustedDevice,
                authSecret: "lan-secret",
                transport: transport
            )
            XCTFail("Expected revoked LAN peer to be rejected")
        } catch {
            XCTAssertEqual(error as? SyncLanHTTPTransportError, .revokedTrustedPeer)
            XCTAssertTrue(transport.requests.isEmpty)
        }
    }

    func testSyncRetryExecutorRecordsAcceptedPushResult() async throws {
        let logbookID = UUID().uuidString
        let events = [
            sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-1"),
            sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-2", previousHash: "event-hash-1")
        ]
        let operationIDs = [UUID().uuidString, UUID().uuidString]
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: operationIDs, events: events)
        )
        let store = await RustBridgeStore(client: client)
        let result = try await store.executeOfflineRetryPush(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            transport: StubSyncPushTransport(response: SyncPushResponse(
                status: "pulled",
                acceptedCount: 2,
                ignoredDuplicateCount: 0,
                rejectedCount: 0,
                serverHeadHash: "event-hash-2",
                errors: []
            ))
        )

        XCTAssertEqual(result.status, .accepted)
        XCTAssertEqual(result.acceptedOperationCount, 2)
        XCTAssertEqual(result.failedOperationCount, 0)
        XCTAssertEqual(client.retryResultPayloads.count, 1)
        XCTAssertEqual(client.retryResultPayloads[0]["result"] as? String, "accepted")
        XCTAssertEqual(client.retryResultPayloads[0]["operation_ids"] as? [String], operationIDs)
        XCTAssertEqual(client.retryResultPayloads[0]["accepted_event_hashes"] as? [String], ["event-hash-1", "event-hash-2"])
    }

    func testSyncRetryExecutorCanUseHostedSyncEndpointStyle() async throws {
        let logbookID = UUID().uuidString
        let event = sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-hosted")
        let operationID = UUID().uuidString
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: [operationID], events: [event])
        )
        let transport = StubSyncPushTransport(response: SyncPushResponse(
            status: "pulled",
            acceptedCount: 1,
            ignoredDuplicateCount: 0,
            rejectedCount: 0,
            serverHeadHash: event.eventHash,
            errors: []
        ))
        let store = await RustBridgeStore(client: client)

        let result = try await store.executeOfflineRetryPush(
            serverURL: try XCTUnwrap(URL(string: "https://api.example.test")),
            syncToken: "sync-secret",
            endpointStyle: SyncPushEndpointStyle(setting: "hosted_sync"),
            transport: transport
        )

        XCTAssertEqual(result.status, .accepted)
        XCTAssertEqual(transport.requests.first?.endpointStyle, .hostedSync)
        XCTAssertEqual(transport.requests.first?.syncToken, "sync-secret")
        XCTAssertEqual(transport.requests.first?.logbookId, logbookID)
    }

    func testBackgroundSyncRunsConfiguredPullAfterCleanPushWhenAutoPullIsEnabled() async throws {
        let logbookID = "00000000-0000-4000-8000-000000000001"
        let localEvent = sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-local")
        let remoteEvent = sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-remote", previousHash: "event-hash-local")
        let operationID = UUID().uuidString
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: [operationID], events: [localEvent])
        )
        let pushTransport = StubSyncPushTransport(response: SyncPushResponse(
            status: "pulled",
            acceptedCount: 1,
            ignoredDuplicateCount: 0,
            rejectedCount: 0,
            serverHeadHash: localEvent.eventHash,
            errors: []
        ))
        let pullTransport = StubSyncPullTransport(response: pullResponse(logbookID: logbookID, events: [remoteEvent]))
        let store = await RustBridgeStore(client: client)

        let result = try await store.executeBackgroundSync(
            serverURL: try XCTUnwrap(URL(string: "https://api.example.test")),
            syncToken: "sync-secret",
            pushEndpointStyle: SyncPushEndpointStyle(setting: "hosted_sync"),
            pullEndpointStyle: SyncPullEndpointStyle(setting: "hosted_sync"),
            autoPullEnabled: true,
            pushTransport: pushTransport,
            pullTransport: pullTransport
        )

        XCTAssertEqual(result.retryResult.status, .accepted)
        XCTAssertEqual(result.pullResult?.status, .applied)
        XCTAssertTrue(result.taskCompleted)
        XCTAssertEqual(pushTransport.requests.first?.endpointStyle, .hostedSync)
        XCTAssertEqual(pullTransport.requests.first?.endpointStyle, .hostedSync)
        XCTAssertEqual(pullTransport.requests.first?.syncToken, "sync-secret")
        XCTAssertEqual(client.remoteApplyPayloads.count, 1)
        XCTAssertEqual(client.remoteApplyPayloads.first?["logbook_id"] as? String, logbookID)
    }

    func testBackgroundSyncSkipsPullAfterUserActionPushFailure() async throws {
        let logbookID = UUID().uuidString
        let event = sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-auth")
        let operationID = UUID().uuidString
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: [operationID], events: [event])
        )
        let pullTransport = StubSyncPullTransport(response: pullResponse(logbookID: logbookID, events: []))
        let store = await RustBridgeStore(client: client)

        let result = try await store.executeBackgroundSync(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            autoPullEnabled: true,
            pushTransport: StubSyncPushTransport(error: SyncHTTPTransportError.serverRejected(
                statusCode: 401,
                message: "unauthorized"
            )),
            pullTransport: pullTransport
        )

        XCTAssertEqual(result.retryResult.status, .userActionRequired)
        XCTAssertNil(result.pullResult)
        XCTAssertTrue(pullTransport.requests.isEmpty)
        XCTAssertEqual(client.retryResultPayloads.first?["result"] as? String, "auth_failed")
    }

    func testSyncRetryExecutorRecordsAuthFailureWithoutLeakingToken() async throws {
        let logbookID = UUID().uuidString
        let event = sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-auth")
        let operationID = UUID().uuidString
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: [operationID], events: [event])
        )
        let store = await RustBridgeStore(client: client)
        let result = try await store.executeOfflineRetryPush(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            transport: StubSyncPushTransport(error: SyncHTTPTransportError.serverRejected(
                statusCode: 401,
                message: "unauthorized"
            ))
        )
        let retryPayloadData = try JSONSerialization.data(withJSONObject: client.retryResultPayloads)
        let retryPayloadJSON = String(decoding: retryPayloadData, as: UTF8.self)

        XCTAssertEqual(result.status, .userActionRequired)
        XCTAssertEqual(client.retryResultPayloads.count, 1)
        XCTAssertEqual(client.retryResultPayloads[0]["result"] as? String, "auth_failed")
        XCTAssertEqual(client.retryResultPayloads[0]["error_code"] as? String, "sync_http_401")
        XCTAssertFalse(retryPayloadJSON.contains("sync-secret"))
    }

    func testSyncRetryExecutorSplitsAcceptedPrefixFromDivergedTail() async throws {
        let logbookID = UUID().uuidString
        let events = [
            sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-prefix"),
            sampleOfficialEvent(logbookID: logbookID, eventHash: "event-hash-diverged", previousHash: "wrong-head")
        ]
        let operationIDs = [UUID().uuidString, UUID().uuidString]
        let client = try RetryExecutorRustBridgeClient(
            retryPlanPayload: retryPlanPayload(operationIds: operationIDs, events: events)
        )
        let store = await RustBridgeStore(client: client)
        let result = try await store.executeOfflineRetryPush(
            serverURL: try XCTUnwrap(URL(string: "https://sync.example.test")),
            syncToken: "sync-secret",
            transport: StubSyncPushTransport(response: SyncPushResponse(
                status: "rejected",
                acceptedCount: 1,
                ignoredDuplicateCount: 0,
                rejectedCount: 1,
                serverHeadHash: "event-hash-prefix",
                errors: ["remote event previous hash does not match expected server head"]
            ))
        )

        XCTAssertEqual(result.status, .partialFailureRecorded)
        XCTAssertEqual(result.acceptedOperationCount, 1)
        XCTAssertEqual(result.failedOperationCount, 1)
        XCTAssertEqual(client.retryResultPayloads.count, 2)
        XCTAssertEqual(client.retryResultPayloads[0]["result"] as? String, "accepted")
        XCTAssertEqual(client.retryResultPayloads[0]["operation_ids"] as? [String], [operationIDs[0]])
        XCTAssertEqual(client.retryResultPayloads[0]["accepted_event_hashes"] as? [String], ["event-hash-prefix"])
        XCTAssertEqual(client.retryResultPayloads[1]["result"] as? String, "diverged")
        XCTAssertEqual(client.retryResultPayloads[1]["operation_ids"] as? [String], [operationIDs[1]])
    }

    func testBackgroundRetryPolicySchedulesConfiguredPendingWork() throws {
        let now = Date(timeIntervalSince1970: 1_785_000_000)
        let policy = SyncBackgroundRetryPolicy(minimumDelaySeconds: 300)
        let decision = policy.decision(
            syncSettings: backgroundSyncSettings(serverURL: "https://sync.example.test", enabled: true),
            pendingChanges: 2,
            hasSyncToken: true,
            now: now
        )

        XCTAssertTrue(decision.shouldSchedule)
        XCTAssertNil(decision.skipReason)
        XCTAssertEqual(decision.earliestBeginDate, now.addingTimeInterval(300))
        XCTAssertEqual(SyncBackgroundRetryTask.identifier, "com.h2technologiesllc.ke8ygw-logger.sync.retry")
        XCTAssertEqual(SyncBackgroundRetryTask.maxMutations, 25)
        XCTAssertEqual(SyncBackgroundRetryTask.backgroundTimeBudgetSeconds, 20)
    }

    func testBackgroundRetryPolicySkipsUnsafeOrUnneededScheduling() throws {
        let policy = SyncBackgroundRetryPolicy(minimumDelaySeconds: 300)

        XCTAssertEqual(
            policy.decision(
                syncSettings: backgroundSyncSettings(serverURL: "https://sync.example.test", enabled: false),
                pendingChanges: 2,
                hasSyncToken: true
            ).skipReason,
            .disabled
        )
        XCTAssertEqual(
            policy.decision(
                syncSettings: backgroundSyncSettings(serverURL: "file:///tmp/sync", enabled: true),
                pendingChanges: 2,
                hasSyncToken: true
            ).skipReason,
            .missingServerURL
        )
        XCTAssertEqual(
            policy.decision(
                syncSettings: backgroundSyncSettings(serverURL: "https://sync.example.test", enabled: true),
                pendingChanges: 2,
                hasSyncToken: false
            ).skipReason,
            .missingSyncToken
        )
        XCTAssertEqual(
            policy.decision(
                syncSettings: backgroundSyncSettings(serverURL: "https://sync.example.test", enabled: true),
                pendingChanges: 0,
                hasSyncToken: true
            ).skipReason,
            .noBackgroundWork
        )
        XCTAssertTrue(
            policy.decision(
                syncSettings: backgroundSyncSettings(serverURL: "https://sync.example.test", enabled: true, autoPullEnabled: true),
                pendingChanges: 0,
                hasSyncToken: true
            ).shouldSchedule
        )
    }

    func testRustBridgeStoreMapsStructuredErrors() async throws {
        let store = await RustBridgeStore(client: ErrorRustBridgeClient())

        do {
            _ = try await store.bridgeSelfTest()
            XCTFail("Expected structured Rust bridge error")
        } catch RustBridgeError.bridge(let code, let message, let correlationID) {
            XCTAssertEqual(code, "invalid_input")
            XCTAssertEqual(message, "bad request")
            XCTAssertEqual(correlationID, "corr-test")
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testFallbackSettingsGetReportsMissingRecord() async throws {
        let client = FallbackRustBridgeClient()
        let result: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.get",
            payload: ["app_support_dir": "swift-test-\(UUID().uuidString)"]
        )

        XCTAssertFalse(result.exists)
        XCTAssertNil(result.settings)
        XCTAssertEqual(result.recordCount, 0)
    }

    func testFallbackSettingsCreateIsIdempotentAndLoadsRecord() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-test-\(UUID().uuidString)"
        let first: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.create_default",
            payload: ["app_support_dir": appSupportDir]
        )
        let second: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.create_default",
            payload: ["app_support_dir": appSupportDir]
        )
        let loaded: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.get",
            payload: ["app_support_dir": appSupportDir]
        )

        XCTAssertTrue(first.exists)
        XCTAssertTrue(first.created)
        XCTAssertFalse(second.created)
        XCTAssertEqual(second.recordCount, 1)
        XCTAssertEqual(loaded.settings?.operator.primaryCallsign, "KE8YGW")
    }

    func testFallbackSettingsUpdateSurvivesReload() async throws {
        let client = FallbackRustBridgeClient()
        let appSupportDir = "swift-test-\(UUID().uuidString)"
        let created: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.create_default",
            payload: ["app_support_dir": appSupportDir]
        )
        var settings = try XCTUnwrap(created.settings)
        settings.operator.primaryCallsign = "k1abc"
        settings.sync.syncServerUrl = "https://sync.example.test"
        let updated: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.update",
            payload: settingsPayload(appSupportDir: appSupportDir, settings: settings)
        )
        let loaded: ApplicationSettingsBridgeResult = try await decodeCommand(
            client: client,
            command: "settings.get",
            payload: ["app_support_dir": appSupportDir]
        )

        XCTAssertEqual(updated.settings?.operator.primaryCallsign, "K1ABC")
        XCTAssertEqual(loaded.settings?.sync.syncServerUrl, "https://sync.example.test")
    }

    func testAppSettingsPayloadDoesNotContainCredentialSecrets() throws {
        let settings = AppSettings()
        settings.setProviderCredentialMetadata("qrz", metadata: [
            "username": "KE8YGW",
            "password_configured": "true"
        ])
        let data = try JSONEncoder().encode(settings.rustSettingsPayload())
        let json = String(decoding: data, as: UTF8.self)

        XCTAssertFalse(json.contains("super-secret"))
        XCTAssertFalse(json.contains("\"password\":\""))
        XCTAssertTrue(json.contains("password_configured"))
    }

    private func decodeCommand<T: Decodable>(
        client: FallbackRustBridgeClient,
        command: String,
        payload: [String: Any]
    ) async throws -> T {
        let request = try JSONSerialization.data(withJSONObject: [
            "command": command,
            "correlation_id": UUID().uuidString,
            "payload": payload
        ])
        let data = try await client.callJSON(request)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(RustBridgeEnvelope<T>.self, from: data)
        return try XCTUnwrap(envelope.data)
    }

    private func settingsPayload(appSupportDir: String, settings: RustApplicationSettings) throws -> [String: Any] {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let settingsData = try encoder.encode(settings)
        let settingsObject = try JSONSerialization.jsonObject(with: settingsData)
        return ["app_support_dir": appSupportDir, "settings": settingsObject]
    }

    private func conflictReportPayload() -> [String: Any] {
        [
            "schema_version": 1,
            "created_at": "2026-07-21T12:00:00Z",
            "logbook_id": UUID().uuidString,
            "peer_id": "ios-fallback-peer",
            "status": "diverged",
            "local_head_hash": "local-head",
            "remote_head_hash": "remote-head",
            "missing_event_count": 1,
            "pending_operation_count": 0,
            "conflicts": [
                [
                    "kind": "divergent_heads",
                    "message": "Local and remote heads diverged.",
                    "related_operation_ids": [],
                    "related_event_hashes": ["local-head", "remote-head"],
                    "safe_auto_merge": false,
                    "requires_user_action": true,
                    "resolution_options": [
                        "keep_local_history",
                        "create_corrective_events",
                        "mark_user_action_required"
                    ]
                ]
            ],
            "recommended_action": "Manual review required before syncing."
        ]
    }

    private func officialEventPayload() -> [String: Any] {
        let logbookID = UUID().uuidString
        return [
            "event_id": UUID().uuidString,
            "event_type": "official.log.qso.created",
            "logbook_id": logbookID,
            "entity_id": UUID().uuidString,
            "previous_hash": NSNull(),
            "event_hash": "event-hash-\(UUID().uuidString)",
            "timestamp": "2026-07-21T12:00:00Z",
            "author_operator_id": NSNull(),
            "station_callsign": "KE8YGW",
            "operator_callsign": "KE8YGW",
            "author_device_id": UUID().uuidString,
            "source_device_id": UUID().uuidString,
            "correlation_id": UUID().uuidString,
            "source_plugin_id": "ios.ke8ygw.logger",
            "schema_version": 1,
            "payload": [
                "contacted_callsign": "K1ABC",
                "rst": 59,
                "portable": true,
                "tags": ["pota", "field"]
            ]
        ]
    }

    private func retryPlanPayload(operationIds: [String], events: [SyncOfficialEvent]) throws -> [String: Any] {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let eventObjects = try events.map { event -> Any in
            let data = try encoder.encode(event)
            return try JSONSerialization.jsonObject(with: data)
        }
        return [
            "schema_version": 1,
            "logbook_id": events.first?.logbookId ?? UUID().uuidString,
            "operation_ids": operationIds,
            "event_hashes": events.map { $0.eventHash },
            "events": eventObjects,
            "missing_local_event_operation_ids": [],
            "network_required": true,
            "blocked_by_network": false,
            "max_mutations": operationIds.count,
            "background_time_budget_seconds": 20,
            "mark_sending": true,
            "permanent_failure_results": ["auth_failed", "validation_failed", "diverged"]
        ]
    }

    private func pullResponse(logbookID: String, events: [SyncOfficialEvent]) -> SyncPullResponse {
        SyncPullResponse(
            preview: SyncPullPreview(
                peerId: "cloud",
                logbookId: logbookID,
                status: events.isEmpty ? "in_sync" : "remote_ahead",
                localHeadHash: nil,
                remoteHeadHash: events.last?.eventHash,
                missingEventCount: events.count,
                remoteEventCount: events.count,
                events: events.map(eventMetadata),
                message: events.isEmpty ? "Local and remote heads match" : "\(events.count) remote events are available to pull"
            ),
            events: events
        )
    }

    private func eventMetadata(_ event: SyncOfficialEvent) -> SyncEventMetadata {
        SyncEventMetadata(
            eventId: event.eventId,
            logbookId: event.logbookId,
            entityId: event.entityId,
            previousHash: event.previousHash,
            eventHash: event.eventHash,
            timestamp: event.timestamp,
            eventType: event.eventType,
            schemaVersion: event.schemaVersion
        )
    }

    private func sampleOfficialEvent(
        logbookID: String = UUID().uuidString,
        eventHash: String = "sample-event-hash",
        previousHash: String? = nil
    ) -> SyncOfficialEvent {
        SyncOfficialEvent(
            eventId: UUID().uuidString,
            eventType: "official.log.qso.created",
            logbookId: logbookID,
            entityId: UUID().uuidString,
            previousHash: previousHash,
            eventHash: eventHash,
            timestamp: "2026-07-21T12:00:00Z",
            authorOperatorId: nil,
            stationCallsign: "KE8YGW",
            operatorCallsign: "KE8YGW",
            authorDeviceId: UUID().uuidString,
            sourceDeviceId: UUID().uuidString,
            correlationId: UUID().uuidString,
            sourcePluginId: "ios.ke8ygw.logger",
            schemaVersion: 1,
            payload: .object([
                "contacted_callsign": .string("K1ABC"),
                "band": .string("20m"),
                "mode": .string("SSB")
            ])
        )
    }

    private func backgroundSyncSettings(serverURL: String, enabled: Bool, autoPullEnabled: Bool = false) -> RustSyncSettings {
        RustSyncSettings(
            syncServerUrl: serverURL,
            deviceName: "KE8YGW Logger iOS",
            syncEndpointStyle: "logbook_scoped",
            preferLanSync: true,
            autoPushEnabled: true,
            autoPullEnabled: autoPullEnabled,
            syncIntervalMinutes: 15,
            backgroundSyncEnabled: enabled,
            accountLabel: nil
        )
    }
}

struct ErrorRustBridgeClient: RustBridgeClient {
    let isLive = false

    func call(_ endpoint: RustBridgeEndpoint, argument: String?) async throws -> Data {
        try await callJSON(Data())
    }

    func callJSON(_ requestData: Data) async throws -> Data {
        let envelope: [String: Any] = [
            "ok": false,
            "bridge_version": 1,
            "abi_version": 1,
            "schema_version": 1,
            "generated_at": "2026-07-10T12:00:00Z",
            "data": NSNull(),
            "error": [
                "code": "invalid_input",
                "message": "bad request",
                "details": [:]
            ],
            "correlation_id": "corr-test"
        ]
        return try JSONSerialization.data(withJSONObject: envelope)
    }
}

private final class RetryExecutorRustBridgeClient: RustBridgeClient {
    let isLive = false
    private let retryPlanPayload: [String: Any]
    private(set) var retryResultPayloads: [[String: Any]] = []
    private(set) var remoteApplyPayloads: [[String: Any]] = []

    init(retryPlanPayload: [String: Any]) throws {
        self.retryPlanPayload = retryPlanPayload
    }

    func call(_ endpoint: RustBridgeEndpoint, argument: String?) async throws -> Data {
        try await FallbackRustBridgeClient().call(endpoint, argument: argument)
    }

    func callJSON(_ requestData: Data) async throws -> Data {
        let request = try JSONSerialization.jsonObject(with: requestData) as? [String: Any]
        let command = request?["command"] as? String
        let correlationID = request?["correlation_id"] as? String ?? "corr-test"
        let payload = request?["payload"] as? [String: Any] ?? [:]

        switch command {
        case "sync.offline_queue.retry_plan":
            return try envelope(data: [
                "retry_plan": retryPlanPayload,
                "offline_queue": FallbackBridgeData.offlineQueue(),
                "recovery": FallbackBridgeData.offlineQueueRecovery()
            ], correlationID: correlationID)
        case "sync.offline_queue.retry_result":
            retryResultPayloads.append(payload)
            return try envelope(data: retryResultResponse(payload: payload), correlationID: correlationID)
        case "sync.snapshot":
            return try envelope(data: FallbackBridgeData.sync(), correlationID: correlationID)
        case "sync.remote_events.apply":
            remoteApplyPayloads.append(payload)
            return try envelope(data: remoteApplyResponse(payload: payload), correlationID: correlationID)
        default:
            return try envelope(
                ok: false,
                data: NSNull(),
                error: ["code": "unsupported_test_command", "message": command ?? "missing command"],
                correlationID: correlationID
            )
        }
    }

    private func retryResultResponse(payload: [String: Any]) -> [String: Any] {
        let result = payload["result"] as? String ?? "transient_failure"
        let operationIDs = payload["operation_ids"] as? [String] ?? []
        let acceptedHashes = payload["accepted_event_hashes"] as? [String] ?? []
        let mutations = operationIDs.enumerated().map { index, operationID in
            mutation(
                operationID: operationID,
                sequence: index + 1,
                result: result,
                errorCode: payload["error_code"] as? String,
                message: payload["message"] as? String
            )
        }

        return [
            "retry_result": [
                "schema_version": 1,
                "logbook_id": retryPlanPayload["logbook_id"] as? String ?? "",
                "result": result,
                "operation_ids": operationIDs,
                "accepted_count": acceptedHashes.count,
                "error_code": payload["error_code"] ?? NSNull(),
                "message": payload["message"] ?? NSNull()
            ],
            "affected_mutations": mutations,
            "offline_queue": FallbackBridgeData.offlineQueue(mutations: mutations)
        ]
    }

    private func remoteApplyResponse(payload: [String: Any]) -> [String: Any] {
        let events = payload["events"] as? [[String: Any]] ?? []
        let remoteHeadHash = events.last?["event_hash"] as? String ?? "remote-head"
        return [
            "sync": FallbackBridgeData.sync(),
            "pull": [
                "schema_version": 1,
                "peer_id": payload["peer_id"] ?? "cloud",
                "logbook_id": payload["logbook_id"] ?? retryPlanPayload["logbook_id"] ?? "",
                "status": events.isEmpty ? "in_sync" : "pulled",
                "accepted_count": events.count,
                "ignored_duplicate_count": 0,
                "rejected_count": 0,
                "local_head_hash": NSNull(),
                "remote_head_hash": remoteHeadHash,
                "errors": [],
                "message": events.isEmpty ? "Local and remote heads match" : "\(events.count) remote events applied"
            ]
        ]
    }

    private func mutation(
        operationID: String,
        sequence: Int,
        result: String,
        errorCode: String?,
        message: String?
    ) -> [String: Any] {
        let status: String
        switch result {
        case "accepted":
            status = "accepted"
        case "transient_failure":
            status = "retrying"
        case "diverged":
            status = "blocked"
        default:
            status = "user_action_required"
        }
        return [
            "operation_id": operationID,
            "logbook_id": retryPlanPayload["logbook_id"] as? String ?? "",
            "entity_id": NSNull(),
            "sequence": sequence,
            "operation_type": "qso.create",
            "status": status,
            "attempts": 1,
            "next_attempt_at": NSNull(),
            "failure_reason": message as Any? ?? NSNull(),
            "last_error_code": errorCode as Any? ?? NSNull(),
            "local_event_hash": NSNull()
        ]
    }

    private func envelope(
        ok: Bool = true,
        data: Any,
        error: Any = NSNull(),
        correlationID: String
    ) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "ok": ok,
            "bridge_version": 1,
            "abi_version": 1,
            "schema_version": 1,
            "generated_at": "2026-07-21T12:00:00Z",
            "data": data,
            "error": error,
            "correlation_id": correlationID
        ])
    }
}

private final class StubSyncPushTransport: SyncPushTransporting {
    struct Request {
        var serverURL: URL
        var bearerToken: String?
        var syncToken: String?
        var endpointStyle: SyncPushEndpointStyle
        var logbookId: String
        var events: [SyncOfficialEvent]
    }

    private let response: SyncPushResponse?
    private let error: Error?
    private(set) var requests: [Request] = []

    init(response: SyncPushResponse) {
        self.response = response
        self.error = nil
    }

    init(error: Error) {
        self.response = nil
        self.error = error
    }

    func push(
        serverURL: URL,
        bearerToken: String?,
        syncToken: String?,
        endpointStyle: SyncPushEndpointStyle,
        logbookId: String,
        events: [SyncOfficialEvent]
    ) async throws -> SyncPushResponse {
        requests.append(Request(
            serverURL: serverURL,
            bearerToken: bearerToken,
            syncToken: syncToken,
            endpointStyle: endpointStyle,
            logbookId: logbookId,
            events: events
        ))
        if let error {
            throw error
        }
        return response ?? SyncPushResponse(
            status: "pulled",
            acceptedCount: events.count,
            ignoredDuplicateCount: 0,
            rejectedCount: 0,
            serverHeadHash: events.last?.eventHash,
            errors: []
        )
    }
}

private final class StubSyncPullTransport: SyncPullTransporting {
    struct Request {
        var serverURL: URL
        var bearerToken: String?
        var syncToken: String?
        var endpointStyle: SyncPullEndpointStyle
        var logbookId: String
        var localHeadHash: String?
    }

    private let response: SyncPullResponse?
    private let error: Error?
    private(set) var requests: [Request] = []

    init(response: SyncPullResponse) {
        self.response = response
        self.error = nil
    }

    init(error: Error) {
        self.response = nil
        self.error = error
    }

    func pull(
        serverURL: URL,
        bearerToken: String?,
        syncToken: String?,
        endpointStyle: SyncPullEndpointStyle,
        logbookId: String,
        localHeadHash: String?
    ) async throws -> SyncPullResponse {
        requests.append(Request(
            serverURL: serverURL,
            bearerToken: bearerToken,
            syncToken: syncToken,
            endpointStyle: endpointStyle,
            logbookId: logbookId,
            localHeadHash: localHeadHash
        ))
        if let error {
            throw error
        }
        return response ?? SyncPullResponse(
            preview: SyncPullPreview(
                peerId: "cloud",
                logbookId: logbookId,
                status: "in_sync",
                localHeadHash: localHeadHash,
                remoteHeadHash: localHeadHash,
                missingEventCount: 0,
                remoteEventCount: 0,
                events: [],
                message: "Local and remote heads match"
            ),
            events: []
        )
    }
}

private final class StubSyncLanPullTransport: SyncLanPullTransporting {
    struct Request {
        var peerURL: URL
        var localIdentity: SyncPeerIdentity
        var trustedDevice: SyncTrustedPeerDevice
        var authSecret: String
        var logbookId: String
        var localHeadHash: String?
    }

    private let response: SyncPullResponse?
    private let error: Error?
    private(set) var requests: [Request] = []

    init(response: SyncPullResponse) {
        self.response = response
        self.error = nil
    }

    init(error: Error) {
        self.response = nil
        self.error = error
    }

    func pull(
        peerURL: URL,
        localIdentity: SyncPeerIdentity,
        trustedDevice: SyncTrustedPeerDevice,
        authSecret: String,
        logbookId: String,
        localHeadHash: String?
    ) async throws -> SyncPullResponse {
        requests.append(Request(
            peerURL: peerURL,
            localIdentity: localIdentity,
            trustedDevice: trustedDevice,
            authSecret: authSecret,
            logbookId: logbookId,
            localHeadHash: localHeadHash
        ))
        if let error {
            throw error
        }
        return response ?? SyncPullResponse(
            preview: SyncPullPreview(
                peerId: trustedDevice.deviceId ?? "lan-peer",
                logbookId: logbookId,
                status: "in_sync",
                localHeadHash: localHeadHash,
                remoteHeadHash: localHeadHash,
                missingEventCount: 0,
                remoteEventCount: 0,
                events: [],
                message: "Local and LAN peer heads match"
            ),
            events: []
        )
    }
}

private final class StubSyncLanPairingTransport: SyncLanPairingTransporting {
    struct Request {
        var peerURL: URL
        var tokenId: String
        var pairingCode: String
        var authSecret: String
        var localIdentity: SyncPeerIdentity
        var logbookId: String
        var publicKeyFingerprint: String?
    }

    private let peerIdentity: SyncPeerIdentity
    private let remoteTrustedDevice: SyncTrustedPeerDevice?
    private let error: Error?
    private(set) var requests: [Request] = []

    init(peerIdentity: SyncPeerIdentity, remoteTrustedDevice: SyncTrustedPeerDevice?) {
        self.peerIdentity = peerIdentity
        self.remoteTrustedDevice = remoteTrustedDevice
        self.error = nil
    }

    init(error: Error) {
        self.peerIdentity = SyncPeerIdentity(
            deviceId: UUID().uuidString,
            sessionId: UUID().uuidString,
            userHash: nil,
            displayName: "LAN Peer",
            capabilities: ["discovery.v1"],
            localApiPort: nil
        )
        self.remoteTrustedDevice = nil
        self.error = error
    }

    func completePairing(
        peerURL: URL,
        tokenId: String,
        pairingCode: String,
        authSecret: String,
        localIdentity: SyncPeerIdentity,
        logbookId: String,
        publicKeyFingerprint: String?
    ) async throws -> SyncLanReciprocalPairingResult {
        requests.append(Request(
            peerURL: peerURL,
            tokenId: tokenId,
            pairingCode: pairingCode,
            authSecret: authSecret,
            localIdentity: localIdentity,
            logbookId: logbookId,
            publicKeyFingerprint: publicKeyFingerprint
        ))
        if let error {
            throw error
        }
        return SyncLanReciprocalPairingResult(
            peerIdentity: peerIdentity,
            remoteTrustedDevice: remoteTrustedDevice
        )
    }
}
