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
        XCTAssertEqual(result.offlineQueue.health.total, 0)
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
