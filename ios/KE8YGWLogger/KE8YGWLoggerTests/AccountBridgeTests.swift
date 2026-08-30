import XCTest
@testable import KE8YGWLogger

/// Simulator-safe coverage for the hosted account bridge. These tests assert
/// that Swift transports and stores only what Rust plans and classifies; no
/// account rule is reimplemented here.
final class AccountBridgeTests: XCTestCase {
    private func encode(_ action: AccountActionRequest) throws -> [String: Any] {
        let data = try AccountCoding.encoder.encode(action)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testUnitActionsEncodeOnlyTheTag() throws {
        let encoded = try encode(.refreshSession())
        XCTAssertEqual(encoded["action"] as? String, "refresh_session")
        XCTAssertEqual(encoded.count, 1)
    }

    func testLoginActionEncodesSnakeCaseWireKeys() throws {
        let encoded = try encode(
            .login(email: "operator@example.com", displayName: nil, deviceName: "iPhone")
        )
        XCTAssertEqual(encoded["action"] as? String, "login")
        XCTAssertEqual(encoded["email"] as? String, "operator@example.com")
        XCTAssertEqual(encoded["device_name"] as? String, "iPhone")
        XCTAssertNil(encoded["display_name"])
    }

    func testRevokeDeviceActionCarriesTheDeviceIdentifier() throws {
        let encoded = try encode(.revokeDevice(deviceId: "device-1"))
        XCTAssertEqual(encoded["action"] as? String, "revoke_device")
        XCTAssertEqual(encoded["device_id"] as? String, "device-1")
    }

    func testAccountSnapshotDecodesRustWireKeys() throws {
        let json = """
        {
          "version": 1,
          "server_url": "https://logger.example",
          "status": "signed_in",
          "hosting": {"operation_mode": "public_hosted", "registration_mode": "open", "turnstile_required": true},
          "account": {"account_id": "a", "user_id": "u", "email": "operator@example.com", "display_name": "Operator", "email_verified_at": "2026-08-01T00:00:00Z"},
          "session": {"session_id": "s", "account_id": "a", "user_id": "u", "device_id": "d", "expires_at": "2026-09-29T00:00:00Z", "active": true},
          "device": {"device_id": "d", "device_name": "iPhone", "revoked": false},
          "logbooks": [{"logbook_id": "l", "name": "Home", "station_callsign": "KE8YGW"}],
          "memberships": [{"logbook_id": "l", "role": "owner"}],
          "devices": [{"device_id": "d", "device_name": "iPhone", "revoked": false}],
          "session_token_credential_id": "cred-1",
          "email_verification_required": false,
          "updated_at": "2026-08-30T12:00:00Z"
        }
        """
        let snapshot = try AccountCoding.decoder.decode(
            AccountSessionSnapshot.self,
            from: Data(json.utf8)
        )
        XCTAssertTrue(snapshot.isSignedIn)
        XCTAssertEqual(snapshot.statusLabel, "Signed in")
        XCTAssertEqual(snapshot.serverUrl, "https://logger.example")
        XCTAssertEqual(snapshot.account?.email, "operator@example.com")
        XCTAssertTrue(snapshot.account?.isEmailVerified == true)
        XCTAssertEqual(snapshot.sessionTokenCredentialId, "cred-1")
        XCTAssertEqual(snapshot.logbooks?.first?.stationCallsign, "KE8YGW")
        XCTAssertEqual(snapshot.memberships?.first?.role, "owner")
        XCTAssertTrue(snapshot.hosting?.allowsOpenRegistration == true)
        XCTAssertTrue(snapshot.hosting?.turnstileRequired == true)
    }

    func testRequestPlanBodyKeysAreNotRewritten() throws {
        let json = """
        {
          "kind": "login",
          "method": "POST",
          "path": "/api/v1/auth/login",
          "url": "https://logger.example/api/v1/auth/login",
          "body": {"email": "operator@example.com", "device_name": "iPhone"},
          "requires_bearer": false,
          "body_secret_fields": [],
          "idempotent": false
        }
        """
        let plan = try AccountCoding.decoder.decode(AccountRequestPlan.self, from: Data(json.utf8))
        guard case .object(let fields)? = plan.body else {
            return XCTFail("planned body should decode as an object")
        }
        XCTAssertEqual(fields["device_name"], .string("iPhone"))
        XCTAssertNil(fields["deviceName"])
        XCTAssertEqual(plan.requiresBearer, false)
    }

    @MainActor
    func testLoginStoresIssuedTokensInTheVaultAndRecordsReferences() async throws {
        let vault = InMemoryCredentialVault()
        let bridge = RustBridgeStore(client: FallbackRustBridgeClient())
        let transport = StubAccountTransport(
            response: AccountTransportResponse(
                status: 200,
                body: .object([
                    "session": .object(["token": .string("session-secret")]),
                    "refresh_token": .string("refresh-secret")
                ]),
                requestId: "req-1",
                transportError: nil
            )
        )
        let service = AccountService(
            transport: transport,
            tokens: AccountTokenVault(vault: vault)
        )
        service.bind(bridge)

        let record = await service.run(.login(email: "operator@example.com"))

        XCTAssertEqual(record?.outcome, "succeeded")
        XCTAssertEqual(transport.lastMethod, "POST")
        XCTAssertNil(transport.lastBearerToken)
        XCTAssertEqual(
            try vault.read(account: AccountTokenVault.sessionAccount, providerId: AccountTokenVault.providerId),
            "session-secret"
        )
        XCTAssertEqual(
            try vault.read(account: AccountTokenVault.refreshAccount, providerId: AccountTokenVault.providerId),
            "refresh-secret"
        )
        XCTAssertNotNil(bridge.account.sessionTokenCredentialId)
    }

    @MainActor
    func testBearerActionsSendTheStoredSessionToken() async throws {
        let vault = InMemoryCredentialVault()
        try vault.save(
            secret: "session-secret",
            account: AccountTokenVault.sessionAccount,
            providerId: AccountTokenVault.providerId
        )
        let bridge = RustBridgeStore(client: FallbackRustBridgeClient())
        let transport = StubAccountTransport(
            response: AccountTransportResponse(status: 200, body: .object([:]), requestId: nil, transportError: nil)
        )
        let service = AccountService(transport: transport, tokens: AccountTokenVault(vault: vault))
        service.bind(bridge)

        _ = await service.run(.listDevices())

        XCTAssertEqual(transport.lastMethod, "GET")
        XCTAssertEqual(transport.lastBearerToken, "session-secret")
    }

    @MainActor
    func testSignOutClearsStoredTokens() async throws {
        let vault = InMemoryCredentialVault()
        try vault.save(
            secret: "session-secret",
            account: AccountTokenVault.sessionAccount,
            providerId: AccountTokenVault.providerId
        )
        try vault.save(
            secret: "refresh-secret",
            account: AccountTokenVault.refreshAccount,
            providerId: AccountTokenVault.providerId
        )
        let bridge = RustBridgeStore(client: FallbackRustBridgeClient())
        let transport = StubAccountTransport(
            response: AccountTransportResponse(status: 200, body: .object(["ok": .bool(true)]), requestId: nil, transportError: nil)
        )
        let service = AccountService(transport: transport, tokens: AccountTokenVault(vault: vault))
        service.bind(bridge)

        _ = await service.run(.logout())

        XCTAssertNil(
            try vault.read(account: AccountTokenVault.sessionAccount, providerId: AccountTokenVault.providerId)
        )
        XCTAssertNil(
            try vault.read(account: AccountTokenVault.refreshAccount, providerId: AccountTokenVault.providerId)
        )
    }

    @MainActor
    func testTransportFailureIsReportedWithoutStoringTokens() async throws {
        let vault = InMemoryCredentialVault()
        let bridge = RustBridgeStore(client: FallbackRustBridgeClient())
        let transport = StubAccountTransport(response: .failure("The network is unavailable."))
        let service = AccountService(transport: transport, tokens: AccountTokenVault(vault: vault))
        service.bind(bridge)

        let record = await service.run(.login(email: "operator@example.com"))

        XCTAssertNotEqual(record?.outcome, "succeeded")
        XCTAssertNil(
            try vault.read(account: AccountTokenVault.sessionAccount, providerId: AccountTokenVault.providerId)
        )
    }

    @MainActor
    func testUnboundServiceReportsAnErrorInsteadOfCrashing() async {
        let service = AccountService(
            transport: StubAccountTransport(response: .failure("unused")),
            tokens: AccountTokenVault(vault: InMemoryCredentialVault())
        )

        let record = await service.run(.refreshSession())

        XCTAssertNil(record)
        XCTAssertNotNil(service.lastError)
    }
}

final class InMemoryCredentialVault: CredentialVault {
    private var storage: [String: String] = [:]

    func save(secret: String, account: String, providerId: String) throws {
        storage["\(providerId):\(account)"] = secret
    }

    func read(account: String, providerId: String) throws -> String? {
        storage["\(providerId):\(account)"]
    }

    func delete(account: String, providerId: String) throws {
        storage.removeValue(forKey: "\(providerId):\(account)")
    }
}

final class StubAccountTransport: AccountHTTPTransport, @unchecked Sendable {
    private let response: AccountTransportResponse
    private(set) var lastMethod: String?
    private(set) var lastURL: String?
    private(set) var lastBearerToken: String?
    private(set) var lastBody: Data?

    init(response: AccountTransportResponse) {
        self.response = response
    }

    func execute(
        method: String,
        url: String,
        bearerToken: String?,
        body: Data?,
        requestId: String
    ) async -> AccountTransportResponse {
        lastMethod = method
        lastURL = url
        lastBearerToken = bearerToken
        lastBody = body
        return response
    }
}
