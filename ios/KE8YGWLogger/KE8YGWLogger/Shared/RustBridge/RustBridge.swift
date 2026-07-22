import Foundation

#if os(iOS)
import Darwin
#endif

enum RustBridgeError: LocalizedError {
    case unavailable(String)
    case bridge(code: String, message: String, correlationID: String?)
    case invalidResponse
    case incompatibleSchema(String)

    var errorDescription: String? {
        switch self {
        case .unavailable(let message):
            return message
        case .bridge(let code, let message, let correlationID):
            if let correlationID {
                return "\(message) (\(code), \(correlationID))"
            }
            return "\(message) (\(code))"
        case .invalidResponse:
            return "The Rust bridge returned an invalid response."
        case .incompatibleSchema(let message):
            return message
        }
    }
}

enum RustBridgeEndpoint: Sendable {
    case version
    case dashboard
    case stationBook
    case providers
    case map
    case sync
    case diagnostics
    case lookupCallsign
    case gridInfo
    case parseADIF
    case exportADIF

    var command: String {
        switch self {
        case .version: return "version"
        case .dashboard: return "dashboard.snapshot"
        case .stationBook: return "station.book"
        case .providers: return "provider.status"
        case .map: return "map.snapshot"
        case .sync: return "sync.snapshot"
        case .diagnostics: return "diagnostics.snapshot"
        case .lookupCallsign: return "lookup.callsign"
        case .gridInfo: return "grid.info"
        case .parseADIF: return "adif.parse"
        case .exportADIF: return "adif.export"
        }
    }

    var symbol: String {
        switch self {
        case .version: return "ham_ios_version_json"
        case .dashboard: return "ham_ios_dashboard_snapshot_json"
        case .stationBook: return "ham_ios_station_book_json"
        case .providers: return "ham_ios_provider_status_json"
        case .map: return "ham_ios_map_snapshot_json"
        case .sync: return "ham_ios_sync_snapshot_json"
        case .diagnostics: return "ham_ios_diagnostics_json"
        case .lookupCallsign: return "ham_ios_lookup_callsign_json"
        case .gridInfo: return "ham_ios_grid_info_json"
        case .parseADIF: return "ham_ios_parse_adif_json"
        case .exportADIF: return "ham_ios_export_adif_json"
        }
    }

    var needsStringArgument: Bool {
        switch self {
        case .lookupCallsign, .gridInfo, .parseADIF, .exportADIF:
            return true
        default:
            return false
        }
    }
}

protocol RustBridgeClient {
    var isLive: Bool { get }
    func call(_ endpoint: RustBridgeEndpoint, argument: String?) async throws -> Data
    func callJSON(_ requestData: Data) async throws -> Data
}

struct RustBridgeEnvelope<T: Decodable>: Decodable {
    let ok: Bool
    let bridgeVersion: Int
    let abiVersion: Int?
    let schemaVersion: Int?
    let generatedAt: String
    let data: T?
    let error: RustBridgeEnvelopeError?
    let correlationId: String?
}

struct RustBridgeEnvelopeError: Decodable {
    let code: String
    let message: String
}

@MainActor
final class RustBridgeStore: ObservableObject {
    @Published var version = BridgeVersion.placeholder
    @Published var dashboard = DashboardSnapshot.placeholder
    @Published var stationBook = StationBookSnapshot.placeholder
    @Published var providers = ProviderStatusSnapshot.placeholder
    @Published var map = MapSnapshot.placeholder
    @Published var sync = SyncSnapshot.placeholder
    @Published var diagnostics = DiagnosticsSnapshot.placeholder
    @Published var lastError: String?

    let client: RustBridgeClient
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder

    init(client: RustBridgeClient = RustBridgeClientFactory.make()) {
        self.client = client
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        self.decoder = decoder
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        self.encoder = encoder
    }

    func refreshAll() async {
        await refreshVersion()
        await refreshDashboard()
        await refreshStationBook()
        await refreshProviders()
        await refreshMap()
        await refreshSync()
        await refreshDiagnostics()
    }

    func refreshVersion() async {
        await assign(endpoint: .version, to: \.version, as: BridgeVersion.self)
    }

    func refreshDashboard() async {
        await assign(endpoint: .dashboard, to: \.dashboard, as: DashboardSnapshot.self)
    }

    func refreshStationBook() async {
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            stationBook = try await command("station.book", payload: AppSupportBridgeRequest(appSupportDir: supportURL.path), as: StationBookSnapshot.self)
            lastError = nil
        } catch {
            await assign(endpoint: .stationBook, to: \.stationBook, as: StationBookSnapshot.self)
        }
    }

    func refreshProviders() async {
        await assign(endpoint: .providers, to: \.providers, as: ProviderStatusSnapshot.self)
    }

    func refreshMap() async {
        await assign(endpoint: .map, to: \.map, as: MapSnapshot.self)
    }

    func refreshSync() async {
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            sync = try await command("sync.snapshot", payload: AppSupportBridgeRequest(appSupportDir: supportURL.path), as: SyncSnapshot.self)
            lastError = nil
        } catch {
            await assign(endpoint: .sync, to: \.sync, as: SyncSnapshot.self)
        }
    }

    func refreshDiagnostics() async {
        do {
            let supportURL = try RustBridgePaths.applicationSupportDirectory()
            diagnostics = try await command("diagnostics.snapshot", payload: AppSupportBridgeRequest(appSupportDir: supportURL.path), as: DiagnosticsSnapshot.self)
            lastError = nil
        } catch {
            await assign(endpoint: .diagnostics, to: \.diagnostics, as: DiagnosticsSnapshot.self)
        }
    }

    func lookup(callsign: String) async throws -> CallsignLookupPayload {
        try await request(.lookupCallsign, as: CallsignLookupPayload.self, argument: callsign)
    }

    func exportADIF(payloads: String) async throws -> ADIFExportPayload {
        try await request(.exportADIF, as: ADIFExportPayload.self, argument: payloads)
    }

    func createQSO(_ request: CreateQSOBridgeRequest) async throws -> QSOBridgeMutationResult {
        try await command("qso.create", payload: request, as: QSOBridgeMutationResult.self)
    }

    func deleteQSO(_ request: DeleteQSOBridgeRequest) async throws -> QSOBridgeMutationResult {
        try await command("qso.delete", payload: request, as: QSOBridgeMutationResult.self)
    }

    func createStationProfile(_ request: StationProfileMutationRequest) async throws -> StationBookMutationResult {
        try await command("station.profile.create", payload: request, as: StationBookMutationResult.self)
    }

    func createStationEquipment(_ request: StationEquipmentMutationRequest) async throws -> StationBookMutationResult {
        try await command("station.equipment.create", payload: request, as: StationBookMutationResult.self)
    }

    func selectStationProfile(_ request: SelectStationProfileBridgeRequest) async throws -> StationBookMutationResult {
        try await command("station.profile.select", payload: request, as: StationBookMutationResult.self)
    }

    func bridgeSelfTest() async throws -> BridgeSelfTestResult {
        try await command("bridge.self_test", payload: EmptyRustBridgePayload(), as: BridgeSelfTestResult.self)
    }

    func startActivation(_ request: ActivationBridgeRequest) async throws -> DomainMutationResult {
        try await command("activation.start", payload: request, as: DomainMutationResult.self)
    }

    func endActivation(_ request: ActivationEndBridgeRequest) async throws -> DomainMutationResult {
        try await command("activation.end", payload: request, as: DomainMutationResult.self)
    }

    func startNetSession(_ request: NetSessionStartBridgeRequest) async throws -> DomainMutationResult {
        try await command("net.session.start", payload: request, as: DomainMutationResult.self)
    }

    func endNetSession(_ request: NetSessionEndBridgeRequest) async throws -> DomainMutationResult {
        try await command("net.session.end", payload: request, as: DomainMutationResult.self)
    }

    func createNetCheckIn(_ request: NetCheckInBridgeRequest) async throws -> DomainMutationResult {
        try await command("net.checkin.create", payload: request, as: DomainMutationResult.self)
    }

    func createNetTraffic(_ request: NetTrafficBridgeRequest) async throws -> DomainMutationResult {
        try await command("net.traffic.create", payload: request, as: DomainMutationResult.self)
    }

    func loadSettings() async throws -> ApplicationSettingsBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        return try await command("settings.get", payload: AppSupportBridgeRequest(appSupportDir: supportURL.path), as: ApplicationSettingsBridgeResult.self)
    }

    func createDefaultSettings() async throws -> ApplicationSettingsBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        return try await command("settings.create_default", payload: AppSupportBridgeRequest(appSupportDir: supportURL.path), as: ApplicationSettingsBridgeResult.self)
    }

    func saveSettings(_ settings: RustApplicationSettings) async throws -> ApplicationSettingsBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        let request = ApplicationSettingsUpdateBridgeRequest(appSupportDir: supportURL.path, settings: settings)
        return try await command("settings.update", payload: request, as: ApplicationSettingsBridgeResult.self)
    }

    func recoverOfflineQueue() async throws -> SyncRecoveryBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        let result = try await command(
            "sync.offline_queue.recover",
            payload: AppSupportBridgeRequest(appSupportDir: supportURL.path),
            as: SyncRecoveryBridgeResult.self
        )
        sync = sync.replacingOfflineQueue(result.offlineQueue)
        lastError = nil
        return result
    }

    func planOfflineRetry(
        maxMutations: Int = 25,
        markSending: Bool = true,
        networkAvailable: Bool = true,
        backgroundTimeBudgetSeconds: Int = 25
    ) async throws -> SyncRetryPlanBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        let request = SyncRetryPlanBridgeRequest(
            appSupportDir: supportURL.path,
            logbookId: nil,
            maxMutations: maxMutations,
            markSending: markSending,
            networkAvailable: networkAvailable,
            backgroundTimeBudgetSeconds: backgroundTimeBudgetSeconds
        )
        let result = try await command("sync.offline_queue.retry_plan", payload: request, as: SyncRetryPlanBridgeResult.self)
        sync = sync.replacingOfflineQueue(result.offlineQueue)
        lastError = nil
        return result
    }

    func recordOfflineRetryResult(
        operationIds: [String],
        acceptedEventHashes: [String] = [],
        result: SyncRetryResultKind,
        errorCode: String? = nil,
        message: String? = nil
    ) async throws -> SyncRetryResultBridgeResult {
        let supportURL = try RustBridgePaths.applicationSupportDirectory()
        let request = SyncRetryResultBridgeRequest(
            appSupportDir: supportURL.path,
            logbookId: nil,
            operationIds: operationIds,
            acceptedEventHashes: acceptedEventHashes,
            result: result,
            errorCode: errorCode,
            message: message
        )
        let response = try await command("sync.offline_queue.retry_result", payload: request, as: SyncRetryResultBridgeResult.self)
        sync = sync.replacingOfflineQueue(response.offlineQueue)
        lastError = nil
        return response
    }

    private func assign<T: Decodable>(
        endpoint: RustBridgeEndpoint,
        to keyPath: ReferenceWritableKeyPath<RustBridgeStore, T>,
        as type: T.Type
    ) async {
        do {
            self[keyPath: keyPath] = try await request(endpoint, as: type)
            lastError = nil
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func request<T: Decodable>(
        _ endpoint: RustBridgeEndpoint,
        as type: T.Type,
        argument: String? = nil
    ) async throws -> T {
        let data = try await client.call(endpoint, argument: argument)
        let envelope = try decoder.decode(RustBridgeEnvelope<T>.self, from: data)
        guard envelope.ok else {
            throw RustBridgeError.bridge(
                code: envelope.error?.code ?? "unknown",
                message: envelope.error?.message ?? "Rust bridge request failed.",
                correlationID: envelope.correlationId
            )
        }
        try validateCompatibility(envelope)
        guard let payload = envelope.data else {
            throw RustBridgeError.invalidResponse
        }
        return payload
    }

    private func command<P: Encodable, T: Decodable>(
        _ command: String,
        payload: P,
        as type: T.Type
    ) async throws -> T {
        let correlationID = UUID().uuidString
        let request = RustBridgeCommandEnvelope(command: command, correlationId: correlationID, payload: payload)
        let requestData = try encoder.encode(request)
        let data = try await client.callJSON(requestData)
        let envelope = try decoder.decode(RustBridgeEnvelope<T>.self, from: data)
        guard envelope.ok else {
            throw RustBridgeError.bridge(
                code: envelope.error?.code ?? "unknown",
                message: envelope.error?.message ?? "Rust bridge request failed.",
                correlationID: envelope.correlationId ?? correlationID
            )
        }
        try validateCompatibility(envelope)
        guard let payload = envelope.data else {
            throw RustBridgeError.invalidResponse
        }
        return payload
    }

    private func validateCompatibility<T>(_ envelope: RustBridgeEnvelope<T>) throws {
        if let abiVersion = envelope.abiVersion, abiVersion != 1 {
            throw RustBridgeError.incompatibleSchema("Unsupported Rust ABI version \(abiVersion).")
        }
        if let schemaVersion = envelope.schemaVersion, schemaVersion != 1 {
            throw RustBridgeError.incompatibleSchema("Unsupported Rust bridge schema version \(schemaVersion).")
        }
    }
}

struct RustBridgeCommandEnvelope<P: Encodable>: Encodable {
    var command: String
    var correlationId: String
    var payload: P
}

struct EmptyRustBridgePayload: Codable {}

struct AppSupportBridgeRequest: Codable {
    var appSupportDir: String
}

enum RustBridgeClientFactory {
    static func make() -> RustBridgeClient {
        if let client = DynamicRustBridgeClient() {
            return client
        }
        return FallbackRustBridgeClient()
    }
}

final class DynamicRustBridgeClient: RustBridgeClient {
    let isLive = true

    init?() {
        #if os(iOS)
        guard ham_ios_abi_version() == 1 else { return nil }
        #else
        return nil
        #endif
    }

    func call(_ endpoint: RustBridgeEndpoint, argument: String?) async throws -> Data {
        let payload: [String: String]
        switch endpoint {
        case .lookupCallsign:
            payload = ["callsign": argument ?? ""]
        case .gridInfo:
            payload = ["grid": argument ?? ""]
        case .parseADIF:
            payload = ["adif": argument ?? ""]
        case .exportADIF:
            return try await callJSON(try legacyADIFExportCommand(argument ?? ""))
        default:
            payload = [:]
        }
        let envelope = RustBridgeCommandEnvelope(command: endpoint.command, correlationId: UUID().uuidString, payload: payload)
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return try await callJSON(try encoder.encode(envelope))
    }

    func callJSON(_ requestData: Data) async throws -> Data {
        #if os(iOS)
        return try await Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            return try requestData.withUnsafeBytes { rawBuffer -> Data in
                guard let baseAddress = rawBuffer.baseAddress else {
                    throw RustBridgeError.invalidResponse
                }
                let pointer = baseAddress.assumingMemoryBound(to: UInt8.self)
                guard let resultPointer = ham_ios_call_json_bytes(pointer, requestData.count) else {
                    throw RustBridgeError.invalidResponse
                }
                let text = String(cString: resultPointer)
                ham_ios_free_string(resultPointer)
                return Data(text.utf8)
            }
        }.value
        #else
        throw RustBridgeError.unavailable("Rust bridge is only loaded on iOS builds.")
        #endif
    }

    private func legacyADIFExportCommand(_ argument: String) throws -> Data {
        let object: [String: Any] = [
            "command": RustBridgeEndpoint.exportADIF.command,
            "correlation_id": UUID().uuidString,
            "payload": ["records": try JSONSerialization.jsonObject(with: Data(argument.utf8))]
        ]
        return try JSONSerialization.data(withJSONObject: object)
    }
}

#if os(iOS)
@_silgen_name("ham_ios_abi_version")
private func ham_ios_abi_version() -> UInt32

@_silgen_name("ham_ios_call_json_bytes")
private func ham_ios_call_json_bytes(_ ptr: UnsafePointer<UInt8>?, _ len: Int) -> UnsafeMutablePointer<CChar>?

@_silgen_name("ham_ios_free_string")
private func ham_ios_free_string(_ ptr: UnsafeMutablePointer<CChar>?)
#endif

struct FallbackRustBridgeClient: RustBridgeClient {
    let isLive = false

    func call(_ endpoint: RustBridgeEndpoint, argument: String?) async throws -> Data {
        let data: [String: Any]
        switch endpoint {
        case .version:
            data = [
                "app": "KE8YGW Logger",
                "core_version": "0.3.0",
                "bridge_version": 1,
                "rust_modules": ["ham-core", "ham-sync", "ham-plugin-sdk"],
                "contract": "ffi_unavailable_in_this_build"
            ]
        case .dashboard:
            data = FallbackBridgeData.dashboard
        case .stationBook:
            data = FallbackBridgeData.stationBook
        case .providers:
            data = FallbackBridgeData.providers
        case .map:
            data = FallbackBridgeData.map
        case .sync:
            data = FallbackBridgeData.sync
        case .diagnostics:
            data = FallbackBridgeData.diagnostics
        case .lookupCallsign:
            data = FallbackBridgeData.lookup(callsign: argument ?? "")
        case .gridInfo:
            data = ["grid": argument ?? "", "valid": false]
        case .parseADIF:
            data = ["records": []]
        case .exportADIF:
            data = ["adif": "Generated by KE8YGW Logger iOS fallback\n<ADIF_VER:5>3.1.4\n<EOH>\n"]
        }

        let envelope: [String: Any] = [
            "ok": true,
            "bridge_version": 1,
            "abi_version": 1,
            "schema_version": 1,
            "generated_at": ISO8601DateFormatter().string(from: Date()),
            "correlation_id": UUID().uuidString,
            "error": NSNull(),
            "data": data
        ]
        return try JSONSerialization.data(withJSONObject: envelope)
    }

    func callJSON(_ requestData: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: requestData) as? [String: Any]
        let command = object?["command"] as? String ?? ""
        let correlationID = object?["correlation_id"] as? String ?? UUID().uuidString
        let payload = object?["payload"] as? [String: Any] ?? [:]
        let data: [String: Any]

        switch command {
        case "settings.get":
            let appSupportDir = payload["app_support_dir"] as? String ?? "fallback"
            data = FallbackSettingsMemory.result(appSupportDir: appSupportDir, createIfMissing: false)
        case "settings.create_default":
            let appSupportDir = payload["app_support_dir"] as? String ?? "fallback"
            data = FallbackSettingsMemory.result(appSupportDir: appSupportDir, createIfMissing: true)
        case "settings.update":
            let appSupportDir = payload["app_support_dir"] as? String ?? "fallback"
            let settings = payload["settings"] as? [String: Any] ?? FallbackSettingsMemory.defaultSettings()
            data = FallbackSettingsMemory.save(appSupportDir: appSupportDir, settings: settings)
        case "qso.create":
            let qso = payload["qso"] as? [String: Any] ?? [:]
            let qsoID = UUID().uuidString
            data = [
                "accepted": true,
                "idempotent": false,
                "official_event": [
                    "event_id": UUID().uuidString,
                    "event_type": "official.log.qso.created",
                    "entity_id": qsoID,
                    "event_hash": "fallback-event-hash",
                    "correlation_id": correlationID,
                    "schema_version": 1,
                    "timestamp": ISO8601DateFormatter().string(from: Date())
                ],
                "qso": [
                    "qso_id": qsoID,
                    "payload": qso,
                    "deleted": false,
                    "last_event_hash": "fallback-event-hash",
                    "schema_version": 1,
                    "projection_source": "fallback"
                ],
                "projection": [
                    "source": "fallback",
                    "schema_version": 1,
                    "last_rust_revision": "fallback-event-hash",
                    "pending_event_count": 1
                ],
                "sync": [
                    "pending_event_count": 1,
                    "authority": "ham-sync"
                ]
            ]
        case "qso.delete":
            let qsoID = payload["qso_id"] as? String ?? UUID().uuidString
            data = [
                "accepted": true,
                "idempotent": false,
                "official_event": [
                    "event_id": UUID().uuidString,
                    "event_type": "official.log.qso.deleted",
                    "entity_id": qsoID,
                    "event_hash": "fallback-delete-hash",
                    "correlation_id": correlationID,
                    "schema_version": 1,
                    "timestamp": ISO8601DateFormatter().string(from: Date())
                ],
                "qso": [
                    "qso_id": qsoID,
                    "payload": [:],
                    "deleted": true,
                    "last_event_hash": "fallback-delete-hash",
                    "schema_version": 1,
                    "projection_source": "fallback"
                ],
                "projection": [
                    "source": "fallback",
                    "schema_version": 1,
                    "last_rust_revision": "fallback-delete-hash",
                    "pending_event_count": 1
                ]
            ]
        case "station.profile.create":
            let profileID = payload["station_profile_id"] as? String ?? UUID().uuidString
            let profile: [String: Any] = [
                "station_profile_id": profileID,
                "display_name": payload["display_name"] as? String ?? "Fallback Station",
                "station_callsign": payload["station_callsign"] as? String ?? "KE8YGW",
                "operator_callsign": payload["operator_callsign"] as? String ?? "KE8YGW",
                "default_grid": payload["default_grid"] as? String ?? "",
                "default_qth": payload["default_qth"] as? String ?? "",
                "default_power_watts": payload["default_power_watts"] as? Int ?? 100,
                "tags": [payload["profile_type"] as? String ?? "home"],
                "active": payload["active"] as? Bool ?? false
            ]
            data = [
                "profile": profile,
                "station_book": [
                    "profiles": [profile],
                    "equipment": [],
                    "configurations": [],
                    "active_profile_id": profileID
                ],
                "idempotent": false,
                "projection_source": "fallback"
            ]
        case "station.equipment.create":
            let equipmentID = payload["equipment_id"] as? String ?? UUID().uuidString
            let equipment: [String: Any] = [
                "equipment_id": equipmentID,
                "equipment_type": payload["equipment_type"] as? String ?? "radio",
                "display_name": payload["display_name"] as? String ?? "Fallback Equipment",
                "manufacturer": payload["manufacturer"] as? String ?? "",
                "model": payload["model"] as? String ?? "",
                "capabilities": payload["capabilities"] as? [String] ?? [],
                "status": "active"
            ]
            data = [
                "equipment": equipment,
                "station_book": [
                    "profiles": [],
                    "equipment": [equipment],
                    "configurations": []
                ],
                "idempotent": false,
                "projection_source": "fallback"
            ]
        case "station.profile.select":
            data = [
                "station_book": FallbackBridgeData.stationBook,
                "projection_source": "fallback"
            ]
        case "sync.offline_queue.recover":
            data = [
                "recovered_count": 0,
                "recovery": FallbackBridgeData.offlineQueueRecovery(),
                "offline_queue": FallbackBridgeData.offlineQueue()
            ]
        case "sync.offline_queue.retry_plan":
            let networkAvailable = payload["network_available"] as? Bool ?? true
            let maxMutations = payload["max_mutations"] as? Int ?? 25
            let backgroundBudget = payload["background_time_budget_seconds"] as? Int ?? 25
            data = [
                "retry_plan": [
                    "schema_version": 1,
                    "logbook_id": payload["logbook_id"] as? String ?? UUID().uuidString,
                    "operation_ids": [],
                    "event_hashes": [],
                    "missing_local_event_operation_ids": [],
                    "network_required": true,
                    "blocked_by_network": !networkAvailable,
                    "max_mutations": maxMutations,
                    "background_time_budget_seconds": backgroundBudget,
                    "mark_sending": networkAvailable ? (payload["mark_sending"] as? Bool ?? true) : false,
                    "permanent_failure_results": [
                        "auth_failed",
                        "validation_failed",
                        "diverged",
                        "missing_local_event",
                        "permanent_failure"
                    ]
                ],
                "offline_queue": FallbackBridgeData.offlineQueue(),
                "recovery": FallbackBridgeData.offlineQueueRecovery()
            ]
        case "sync.offline_queue.retry_result":
            let operationIDs = payload["operation_ids"] as? [String] ?? []
            let result = payload["result"] as? String ?? "accepted"
            let acceptedHashes = payload["accepted_event_hashes"] as? [String] ?? []
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
            let errorCode = payload["error_code"] as? String ?? result
            let mutations = operationIDs.map { operationID in
                FallbackBridgeData.offlineMutation(
                    operationId: operationID,
                    status: status,
                    operationType: "qso.create",
                    lastErrorCode: result == "accepted" ? nil : errorCode
                )
            }
            data = [
                "retry_result": [
                    "schema_version": 1,
                    "logbook_id": payload["logbook_id"] as? String ?? UUID().uuidString,
                    "result": result,
                    "operation_ids": operationIDs,
                    "accepted_count": result == "accepted" ? acceptedHashes.count : 0,
                    "error_code": errorCode,
                    "message": payload["message"] as? String ?? result
                ],
                "affected_mutations": mutations,
                "offline_queue": FallbackBridgeData.offlineQueue(mutations: mutations)
            ]
        case "bridge.self_test":
            data = [
                "success": true,
                "library_linked": false,
                "abi_version": 1,
                "bridge_schema_version": 1,
                "core_version": "0.3.0",
                "sync_protocol_version": 1,
                "backup_schema_version": 1,
                "build_target": ["os": "fallback", "arch": "fallback"],
                "json_round_trip": true,
                "error_round_trip": true,
                "allocation_model": "fallback_no_rust_allocation"
            ]
        case "activation.start", "activation.end", "net.session.start", "net.session.end", "net.checkin.create", "net.traffic.create":
            let eventType: String
            switch command {
            case "activation.start": eventType = "official.log.activation.started"
            case "activation.end": eventType = "official.log.activation.ended"
            case "net.session.start": eventType = "official.log.net.session.started"
            case "net.session.end": eventType = "official.log.net.session.ended"
            case "net.checkin.create": eventType = "official.log.net.checkin.created"
            default: eventType = "official.log.net.traffic.created"
            }
            data = [
                "accepted": true,
                "official_event": [
                    "event_id": UUID().uuidString,
                    "event_type": eventType,
                    "entity_id": payload["activation_id"] as? String ?? payload["net_session_id"] as? String ?? UUID().uuidString,
                    "event_hash": "fallback-domain-hash",
                    "correlation_id": correlationID,
                    "schema_version": 1,
                    "timestamp": ISO8601DateFormatter().string(from: Date())
                ],
                "projection": [
                    "source": "fallback",
                    "schema_version": 1,
                    "pending_event_count": 1
                ]
            ]
        default:
            return try await call(endpointForCommand(command), argument: nil)
        }

        let envelope: [String: Any] = [
            "ok": true,
            "bridge_version": 1,
            "abi_version": 1,
            "schema_version": 1,
            "generated_at": ISO8601DateFormatter().string(from: Date()),
            "correlation_id": correlationID,
            "error": NSNull(),
            "data": data
        ]
        return try JSONSerialization.data(withJSONObject: envelope)
    }

    private func endpointForCommand(_ command: String) -> RustBridgeEndpoint {
        switch command {
        case RustBridgeEndpoint.dashboard.command: return .dashboard
        case RustBridgeEndpoint.stationBook.command: return .stationBook
        case RustBridgeEndpoint.providers.command: return .providers
        case RustBridgeEndpoint.map.command: return .map
        case RustBridgeEndpoint.sync.command: return .sync
        case RustBridgeEndpoint.diagnostics.command: return .diagnostics
        default: return .version
        }
    }
}

private enum FallbackSettingsMemory {
    private static let lock = NSLock()
    private static var records: [String: [String: Any]] = [:]

    static func result(appSupportDir: String, createIfMissing: Bool) -> [String: Any] {
        lock.lock()
        defer { lock.unlock() }
        if let settings = records[appSupportDir] {
            return ["exists": true, "created": false, "settings": settings, "record_count": 1]
        }
        guard createIfMissing else {
            return ["exists": false, "created": false, "settings": NSNull(), "record_count": 0]
        }
        let settings = defaultSettings()
        records[appSupportDir] = settings
        return ["exists": true, "created": true, "settings": settings, "record_count": 1]
    }

    static func save(appSupportDir: String, settings: [String: Any]) -> [String: Any] {
        lock.lock()
        defer { lock.unlock() }
        let normalized = normalizedSettings(settings)
        records[appSupportDir] = normalized
        return ["exists": true, "created": false, "settings": normalized, "record_count": 1]
    }

    static func defaultSettings() -> [String: Any] {
        let now = ISO8601DateFormatter().string(from: Date())
        return [
            "schema_version": 1,
            "operator": [
                "primary_callsign": "KE8YGW",
                "additional_callsigns": [],
                "operator_name": "",
                "operator_email": "",
                "station_callsign": "KE8YGW",
                "default_station_profile_id": "",
                "default_equipment_profile_id": ""
            ],
            "location": [
                "use_device_location": true,
                "manual_grid_override_enabled": false,
                "manual_maidenhead_grid": "EN91",
                "last_gps_grid": "",
                "last_location_source": MaidenheadLocationSource.stationDefault.rawValue,
                "manual_location_name": "",
                "manual_county": "",
                "manual_state": "",
                "manual_country": "United States"
            ],
            "providers": [
                "enabled": [:],
                "credential_metadata": [:],
                "validation": [:]
            ],
            "sync": [
                "sync_server_url": "http://127.0.0.1:9740",
                "device_name": "KE8YGW Logger iOS",
                "prefer_lan_sync": true,
                "auto_push_enabled": false,
                "auto_pull_enabled": false,
                "sync_interval_minutes": 15,
                "background_sync_enabled": true,
                "account_label": ""
            ],
            "logging": [
                "default_band": "20m",
                "default_mode": "SSB",
                "auto_uppercase_callsigns": true,
                "ask_for_location_later": false,
                "callsign_lookup_preference": "automatic"
            ],
            "activation": [
                "allow_offline_activations": true,
                "validation_ttl_hours": 24,
                "notes_template": "",
                "pota_upload_enabled": false,
                "sota_upload_enabled": false
            ],
            "net_control": [
                "default_name": "Weekly Emergency Net",
                "default_frequency_mhz": "146.520",
                "default_mode": "FM",
                "sort_roster_by_traffic_priority": true
            ],
            "display": [
                "appearance": "system",
                "accent_color_name": "blue",
                "map_default_layer": "Stations",
                "show_qso_map_objects": true,
                "show_station_map_markers": true
            ],
            "backup": ["include_diagnostics_by_default": false],
            "privacy": ["provider_notifications_enabled": true],
            "diagnostics": ["share_diagnostics_with_logs": true],
            "developer": ["developer_mode_enabled": false],
            "created_at": now,
            "updated_at": now
        ]
    }

    private static func normalizedSettings(_ settings: [String: Any]) -> [String: Any] {
        var normalized = settings
        if var op = normalized["operator"] as? [String: Any] {
            op["primary_callsign"] = (op["primary_callsign"] as? String ?? "KE8YGW").trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            op["station_callsign"] = (op["station_callsign"] as? String ?? "KE8YGW").trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            normalized["operator"] = op
        }
        if var logging = normalized["logging"] as? [String: Any] {
            logging["default_mode"] = (logging["default_mode"] as? String ?? "SSB").trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            normalized["logging"] = logging
        }
        if var net = normalized["net_control"] as? [String: Any] {
            net["default_mode"] = (net["default_mode"] as? String ?? "FM").trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            normalized["net_control"] = net
        }
        normalized["updated_at"] = ISO8601DateFormatter().string(from: Date())
        return normalized
    }
}

enum FallbackBridgeData {
    static let dashboard: [String: Any] = [
        "operator": "KE8YGW",
        "active_station": stationProfile,
        "active_configuration": stationConfiguration,
        "current_profile": "Home Station",
        "gps": [
            "available": true,
            "source": "ios-core-location",
            "coordinate": ["latitude": 41.0, "longitude": -81.0],
            "grid": "EN91"
        ],
        "recent_qsos": [],
        "pending_uploads": 0,
        "provider_status": ["providers": []],
        "sync_status": [
            "mode": "offline_first",
            "pending_changes": 0,
            "conflicts": 0
        ],
        "offline": true,
        "battery": ["source": "ios-uidevice", "status": "provided_by_swift"],
        "network": ["source": "ios-network-framework", "status": "provided_by_swift"],
        "capabilities": [
            "casual_logging",
            "portable_logging",
            "pota",
            "sota",
            "net_control",
            "provider_status",
            "maps",
            "diagnostics",
            "hosted_sync_model"
        ]
    ]

    static let stationProfile: [String: Any] = [
        "station_profile_id": UUID().uuidString,
        "display_name": "Home Station",
        "station_callsign": "KE8YGW",
        "operator_callsign": "KE8YGW",
        "default_grid": "EN91",
        "default_qth": "Cleveland, OH",
        "default_power_watts": 100,
        "tags": ["home"],
        "active": true
    ]

    static let stationConfiguration: [String: Any] = [
        "configuration_id": UUID().uuidString,
        "station_profile_id": stationProfile["station_profile_id"] ?? UUID().uuidString,
        "name": "HF Voice/Digital",
        "band_hint": "20m",
        "mode_hint": "SSB",
        "default_power_watts": 100
    ]

    static let stationBook: [String: Any] = [
        "profiles": [
            stationProfile,
            [
                "station_profile_id": UUID().uuidString,
                "display_name": "Portable Station",
                "station_callsign": "KE8YGW/P",
                "operator_callsign": "KE8YGW",
                "default_power_watts": 10,
                "tags": ["portable", "pota", "sota"],
                "active": false
            ]
        ],
        "equipment": [
            [
                "equipment_id": UUID().uuidString,
                "equipment_type": "radio",
                "display_name": "Field HF Rig",
                "manufacturer": "Generic",
                "model": "Portable 100",
                "capabilities": ["hf", "voice", "cw", "digital"],
                "status": "active"
            ],
            [
                "equipment_id": UUID().uuidString,
                "equipment_type": "antenna",
                "display_name": "Linked Dipole",
                "capabilities": ["40m", "20m", "10m"],
                "status": "active"
            ]
        ],
        "configurations": [stationConfiguration],
        "active_profile_id": stationProfile["station_profile_id"] ?? "",
        "active_configuration_id": stationConfiguration["configuration_id"] ?? ""
    ]

    static let providers: [String: Any] = [
        "service_registry": ["providers": []],
        "online_providers": [
            provider("qrz", "QRZ", "credential_required"),
            provider("hamqth", "HamQTH", "credential_required"),
            provider("pota", "POTA", "ready_for_network_adapter"),
            provider("sotawatch", "SOTAWatch", "ready_for_network_adapter"),
            provider("dx-cluster", "DX Cluster", "offline_parser_ready"),
            provider("lotw", "LoTW", "credential_required"),
            provider("eqsl", "eQSL", "credential_required"),
            provider("club-log", "Club Log", "credential_required"),
            provider("qrz-logbook", "QRZ Logbook", "credential_required")
        ],
        "upload_queue": ["targets": [], "jobs": []],
        "api_status": [
            "qrz": "stub_requires_credentials",
            "hamqth": "stub_requires_credentials",
            "pota": "provider_ready_for_network_adapter",
            "sotawatch": "provider_ready_for_network_adapter",
            "dx_cluster": "offline_parser_ready",
            "lotw": "stub_requires_credentials",
            "eqsl": "stub_requires_credentials",
            "club_log": "stub_requires_credentials",
            "qrz_logbook": "stub_requires_credentials"
        ]
    ]

    static let map: [String: Any] = [
        "providers": [],
        "layers": ["layers": []],
        "qso_objects": [],
        "station_markers": [],
        "status": [
            "grid": "EN91",
            "coordinates": ["latitude": 41.0, "longitude": -81.0],
            "distance": "n/a",
            "bearing": "n/a",
            "zoom": "8",
            "selected_layer": "Stations"
        ]
    ]

    static let sync: [String: Any] = [
        "cloud_connection_state": "disconnected",
        "pending_changes": 0,
        "offline_queue": offlineQueue(),
        "conflict_reviews": ["health": ["open": 0, "resolved": 0, "total": 0]],
        "conflicts": [],
        "history": [],
        "retry_policy": [
            "network_required": true,
            "background_retry_supported": true,
            "permanent_user_action_states": ["blocked", "failed", "user_action_required"]
        ]
    ]

    static func offlineQueue(mutations: [[String: Any]] = []) -> [String: Any] {
        let statuses = mutations.compactMap { $0["status"] as? String }
        let pending = statuses.filter { $0 == "pending" }.count
        let sending = statuses.filter { $0 == "sending" }.count
        let retrying = statuses.filter { $0 == "retrying" }.count
        let blocked = statuses.filter { $0 == "blocked" }.count
        let failed = statuses.filter { $0 == "failed" }.count
        let accepted = statuses.filter { $0 == "accepted" }.count
        let userAction = statuses.filter { $0 == "user_action_required" }.count
        return [
            "queue_schema_version": 1,
            "mutation_schema_version": 1,
            "health": [
                "total": mutations.count,
                "pending": pending,
                "sending": sending,
                "retrying": retrying,
                "blocked": blocked,
                "failed": failed,
                "accepted": accepted,
                "user_action_required": userAction,
                "ready_to_send": pending + retrying,
                "oldest_pending_at": NSNull(),
                "newest_update_at": NSNull()
            ],
            "mutations": mutations
        ]
    }

    static func offlineMutation(
        operationId: String,
        status: String,
        operationType: String,
        lastErrorCode: String? = nil
    ) -> [String: Any] {
        var mutation: [String: Any] = [
            "operation_id": operationId,
            "logbook_id": UUID().uuidString,
            "sequence": 1,
            "operation_type": operationType,
            "status": status,
            "attempts": status == "accepted" ? 1 : 0,
            "next_attempt_at": NSNull(),
            "failure_reason": NSNull(),
            "last_error_code": NSNull(),
            "local_event_hash": status == "accepted" ? "fallback-event-hash" : NSNull()
        ]
        if let lastErrorCode {
            mutation["last_error_code"] = lastErrorCode
            mutation["failure_reason"] = lastErrorCode
        }
        return mutation
    }

    static func offlineQueueRecovery() -> [String: Any] {
        [
            "initialized_empty_queue": false,
            "migrated_v0_2_absent_queue": false,
            "migrated_v0_2_file": false,
            "migrated_legacy_mutations": 0,
            "recovered_interrupted_writes": 0,
            "promoted_interrupted_atomic_write": false,
            "quarantined_corrupt_file": false
        ]
    }

    static let diagnostics: [String: Any] = [
        "rust_version": "0.3.0",
        "core_version": "0.3.0",
        "bridge_loaded": false,
        "abi_version": 1,
        "bridge_schema_version": 1,
        "sync_protocol_version": 1,
        "backup_schema_version": 1,
        "database_status": ["official_event_store": "ffi_unavailable"],
        "provider_health": ["providers": []],
        "sync_queue": ["pending_uploads": 0, "pending_sync_events": 0, "conflicts": 0],
        "station": ["profiles": 2, "equipment": 2, "configurations": 1],
        "logs": ["runtime_jsonl": "ham-core runtime log format supported"],
        "report_id": UUID().uuidString
    ]

    static func lookup(callsign: String) -> [String: Any] {
        [
            "callsign": callsign.trimmingCharacters(in: .whitespacesAndNewlines).uppercased(),
            "provider_id": "ios-fallback",
            "source": "ffi_unavailable",
            "result": [
                "name": "",
                "qth": "",
                "country": "",
                "dxcc": NSNull(),
                "cq_zone": NSNull(),
                "itu_zone": NSNull(),
                "grid": NSNull(),
                "license_class": NSNull()
            ]
        ]
    }

    private static func provider(_ id: String, _ name: String, _ status: String) -> [String: Any] {
        [
            "provider_id": id,
            "display_name": name,
            "service_type": "online",
            "required_credentials": [],
            "supports_offline": status.contains("offline"),
            "requires_network_access": true,
            "status": status
        ]
    }
}

struct BridgeVersion: Decodable {
    var app: String
    var coreVersion: String
    var bridgeVersion: Int
    var rustModules: [String]
    var contract: String

    static let placeholder = BridgeVersion(
        app: "KE8YGW Logger",
        coreVersion: "unknown",
        bridgeVersion: 0,
        rustModules: [],
        contract: "not loaded"
    )
}

struct DashboardSnapshot: Decodable {
    var operatorCallsign: String
    var activeStation: StationProfileSnapshot?
    var activeConfiguration: StationConfigurationSnapshot?
    var currentProfile: String
    var gps: GPSSnapshot?
    var recentQsos: [BridgeQSO]
    var pendingUploads: Int
    var syncStatus: SyncSummary?
    var offline: Bool
    var capabilities: [String]

    enum CodingKeys: String, CodingKey {
        case operatorCallsign = "operator"
        case activeStation
        case activeConfiguration
        case currentProfile
        case gps
        case recentQsos
        case pendingUploads
        case syncStatus
        case offline
        case capabilities
    }

    static let placeholder = DashboardSnapshot(
        operatorCallsign: "KE8YGW",
        activeStation: nil,
        activeConfiguration: nil,
        currentProfile: "Local",
        gps: nil,
        recentQsos: [],
        pendingUploads: 0,
        syncStatus: nil,
        offline: true,
        capabilities: []
    )
}

struct GPSSnapshot: Decodable {
    var available: Bool
    var source: String
    var coordinate: BridgeCoordinate?
    var grid: String?
}

struct BridgeCoordinate: Decodable {
    var latitude: Double
    var longitude: Double
}

struct SyncSummary: Decodable {
    var mode: String?
    var pendingChanges: Int?
    var conflicts: Int?
}

struct BridgeQSO: Decodable, Identifiable {
    var id: String { qsoId ?? UUID().uuidString }
    var qsoId: String?
    var callsign: String?
    var band: String?
    var mode: String?
    var startedAt: String?
}

struct StationBookSnapshot: Decodable {
    var profiles: [StationProfileSnapshot]
    var equipment: [EquipmentSnapshot]
    var configurations: [StationConfigurationSnapshot]
    var activeProfileId: String?
    var activeConfigurationId: String?

    static let placeholder = StationBookSnapshot(
        profiles: [],
        equipment: [],
        configurations: [],
        activeProfileId: nil,
        activeConfigurationId: nil
    )
}

struct StationProfileSnapshot: Decodable, Identifiable {
    var id: String { stationProfileId }
    var stationProfileId: String
    var displayName: String
    var stationCallsign: String
    var operatorCallsign: String?
    var defaultGrid: String?
    var defaultQth: String?
    var defaultPowerWatts: Int?
    var tags: [String]?
    var active: Bool?
}

struct EquipmentSnapshot: Decodable, Identifiable {
    var id: String { equipmentId }
    var equipmentId: String
    var equipmentType: String
    var displayName: String
    var manufacturer: String?
    var model: String?
    var capabilities: [String]?
    var status: String?
}

struct StationConfigurationSnapshot: Decodable, Identifiable {
    var id: String { configurationId }
    var configurationId: String
    var stationProfileId: String
    var name: String
    var bandHint: String?
    var modeHint: String?
    var defaultPowerWatts: Int?
}

struct ProviderStatusSnapshot: Decodable {
    var onlineProviders: [ProviderMetadataSnapshot]
    var apiStatus: [String: String]?

    static let placeholder = ProviderStatusSnapshot(onlineProviders: [], apiStatus: nil)
}

struct ProviderMetadataSnapshot: Decodable, Identifiable {
    var id: String { providerId }
    var providerId: String
    var displayName: String
    var serviceType: String?
    var requiredCredentials: [String]?
    var requiredConfigKeys: [String]?
    var supportsOffline: Bool?
    var requiresNetworkAccess: Bool?
    var status: String?
    var enabled: Bool?
}

struct MapSnapshot: Decodable {
    var status: MapStatusSnapshot
    var stationMarkers: [MapMarkerSnapshot]?
    var qsoObjects: [MapMarkerSnapshot]?

    static let placeholder = MapSnapshot(
        status: MapStatusSnapshot(
            grid: "unknown",
            coordinates: BridgeCoordinate(latitude: 0, longitude: 0),
            distance: "n/a",
            bearing: "n/a",
            zoom: "4",
            selectedLayer: "none"
        ),
        stationMarkers: [],
        qsoObjects: []
    )
}

struct MapStatusSnapshot: Decodable {
    var grid: String
    var coordinates: BridgeCoordinate
    var distance: String
    var bearing: String
    var zoom: String
    var selectedLayer: String
}

struct MapMarkerSnapshot: Decodable, Identifiable {
    var id: String { markerId ?? UUID().uuidString }
    var markerId: String?
    var title: String?
}

struct SyncSnapshot: Decodable {
    var cloudConnectionState: String?
    var pendingChanges: Int?
    var offlineQueue: SyncOfflineQueueSnapshot?
    var conflictReviews: SyncConflictReviewSnapshot?
    var conflicts: [String]?
    var history: [String]?
    var retryPolicy: SyncRetryPolicy?

    enum CodingKeys: String, CodingKey {
        case cloudConnectionState
        case pendingChanges
        case offlineQueue
        case conflictReviews
        case conflicts
        case history
        case retryPolicy
    }

    static let placeholder = SyncSnapshot(
        cloudConnectionState: "disconnected",
        pendingChanges: 0,
        offlineQueue: nil,
        conflictReviews: nil,
        conflicts: [],
        history: [],
        retryPolicy: nil
    )

    init(
        cloudConnectionState: String?,
        pendingChanges: Int?,
        offlineQueue: SyncOfflineQueueSnapshot?,
        conflictReviews: SyncConflictReviewSnapshot?,
        conflicts: [String]?,
        history: [String]?,
        retryPolicy: SyncRetryPolicy?
    ) {
        self.cloudConnectionState = cloudConnectionState
        self.pendingChanges = pendingChanges
        self.offlineQueue = offlineQueue
        self.conflictReviews = conflictReviews
        self.conflicts = conflicts
        self.history = history
        self.retryPolicy = retryPolicy
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        cloudConnectionState = try container.decodeIfPresent(String.self, forKey: .cloudConnectionState)
        pendingChanges = try container.decodeIfPresent(Int.self, forKey: .pendingChanges)
        offlineQueue = try? container.decodeIfPresent(SyncOfflineQueueSnapshot.self, forKey: .offlineQueue)
        conflictReviews = try? container.decodeIfPresent(SyncConflictReviewSnapshot.self, forKey: .conflictReviews)
        conflicts = (try? container.decodeIfPresent([String].self, forKey: .conflicts)) ?? []
        history = (try? container.decodeIfPresent([String].self, forKey: .history)) ?? []
        retryPolicy = try? container.decodeIfPresent(SyncRetryPolicy.self, forKey: .retryPolicy)
    }

    func replacingOfflineQueue(_ queue: SyncOfflineQueueSnapshot) -> SyncSnapshot {
        SyncSnapshot(
            cloudConnectionState: cloudConnectionState,
            pendingChanges: queue.health.pendingChangeCount,
            offlineQueue: queue,
            conflictReviews: conflictReviews,
            conflicts: conflicts,
            history: history,
            retryPolicy: retryPolicy
        )
    }
}

struct SyncOfflineQueueSnapshot: Decodable {
    var queueSchemaVersion: Int?
    var mutationSchemaVersion: Int?
    var health: SyncOfflineQueueHealth
    var mutations: [SyncOfflineMutation]
}

struct SyncOfflineQueueHealth: Decodable {
    var total: Int?
    var pending: Int?
    var sending: Int?
    var retrying: Int?
    var blocked: Int?
    var failed: Int?
    var accepted: Int?
    var userActionRequired: Int?
    var readyToSend: Int?
    var oldestPendingAt: String?
    var newestUpdateAt: String?

    var pendingChangeCount: Int {
        (pending ?? 0) + (sending ?? 0) + (retrying ?? 0) + (blocked ?? 0) + (failed ?? 0) + (userActionRequired ?? 0)
    }
}

struct SyncOfflineMutation: Decodable, Identifiable {
    var id: String { operationId ?? "\(logbookId ?? "unknown")-\(sequence ?? 0)" }
    var operationId: String?
    var logbookId: String?
    var entityId: String?
    var sequence: Int?
    var operationType: String?
    var status: String?
    var attempts: Int?
    var nextAttemptAt: String?
    var failureReason: String?
    var lastErrorCode: String?
    var localEventHash: String?
}

struct SyncConflictReviewSnapshot: Decodable {
    var health: SyncConflictReviewHealth?
}

struct SyncConflictReviewHealth: Decodable {
    var open: Int?
    var resolved: Int?
    var total: Int?
}

struct SyncRetryPolicy: Decodable {
    var networkRequired: Bool?
    var backgroundRetrySupported: Bool?
    var permanentUserActionStates: [String]?
}

struct SyncRecoveryBridgeResult: Decodable {
    var recoveredCount: Int?
    var recovery: SyncOfflineQueueRecoveryReport?
    var offlineQueue: SyncOfflineQueueSnapshot
}

struct SyncOfflineQueueRecoveryReport: Decodable {
    var initializedEmptyQueue: Bool?
    var migratedV02AbsentQueue: Bool?
    var migratedV02File: Bool?
    var migratedLegacyMutations: Int?
    var recoveredInterruptedWrites: Int?
    var promotedInterruptedAtomicWrite: Bool?
    var quarantinedCorruptFile: Bool?
}

struct SyncRetryPlanBridgeRequest: Encodable {
    var appSupportDir: String
    var logbookId: String?
    var maxMutations: Int?
    var markSending: Bool
    var networkAvailable: Bool
    var backgroundTimeBudgetSeconds: Int?
}

struct SyncRetryPlanBridgeResult: Decodable {
    var retryPlan: SyncRetryPlan
    var offlineQueue: SyncOfflineQueueSnapshot
    var recovery: SyncOfflineQueueRecoveryReport?
}

struct SyncRetryPlan: Decodable {
    var schemaVersion: Int?
    var logbookId: String?
    var operationIds: [String]
    var eventHashes: [String]
    var missingLocalEventOperationIds: [String]
    var networkRequired: Bool
    var blockedByNetwork: Bool
    var maxMutations: Int?
    var backgroundTimeBudgetSeconds: Int?
    var markSending: Bool
    var permanentFailureResults: [SyncRetryResultKind]?
}

struct SyncRetryResultBridgeRequest: Encodable {
    var appSupportDir: String
    var logbookId: String?
    var operationIds: [String]
    var acceptedEventHashes: [String]
    var result: SyncRetryResultKind
    var errorCode: String?
    var message: String?
}

struct SyncRetryResultBridgeResult: Decodable {
    var retryResult: SyncRetryResultSummary
    var affectedMutations: [SyncOfflineMutation]
    var offlineQueue: SyncOfflineQueueSnapshot
}

struct SyncRetryResultSummary: Decodable {
    var schemaVersion: Int?
    var logbookId: String?
    var result: SyncRetryResultKind
    var operationIds: [String]
    var acceptedCount: Int
    var errorCode: String?
    var message: String?
}

enum SyncRetryResultKind: String, Codable {
    case accepted
    case transientFailure = "transient_failure"
    case authFailed = "auth_failed"
    case validationFailed = "validation_failed"
    case diverged
    case missingLocalEvent = "missing_local_event"
    case permanentFailure = "permanent_failure"
}

struct DiagnosticsSnapshot: Decodable {
    var rustVersion: String
    var coreVersion: String?
    var bridgeLoaded: Bool?
    var abiVersion: Int?
    var bridgeSchemaVersion: Int?
    var syncProtocolVersion: Int?
    var backupSchemaVersion: Int?
    var reportId: String?

    static let placeholder = DiagnosticsSnapshot(
        rustVersion: "unknown",
        coreVersion: nil,
        bridgeLoaded: nil,
        abiVersion: nil,
        bridgeSchemaVersion: nil,
        syncProtocolVersion: nil,
        backupSchemaVersion: nil,
        reportId: nil
    )
}

struct CallsignLookupPayload: Decodable {
    var callsign: String
    var providerId: String
    var source: String
    var result: CallsignLookupResult?
}

struct CallsignLookupResult: Decodable {
    var name: String?
    var qth: String?
    var country: String?
    var dxcc: Int?
    var cqZone: Int?
    var ituZone: Int?
    var grid: String?
    var licenseClass: String?
}

struct ADIFExportPayload: Decodable {
    var adif: String
}

struct CreateQSOBridgeRequest: Codable {
    var appSupportDir: String
    var operationId: String
    var deviceId: String?
    var qso: CreateQSOBridgePayload
}

struct CreateQSOBridgePayload: Codable {
    var contactedCallsign: String
    var stationCallsign: String
    var operatorCallsign: String
    var startedAt: String
    var mode: String
    var band: String? = nil
    var submode: String? = nil
    var frequencyMhz: Double? = nil
    var rstSent: String? = nil
    var rstReceived: String? = nil
    var powerWatts: Double? = nil
    var stationProfileId: String? = nil
    var equipmentSummary: String? = nil
    var grid: String? = nil
    var county: String? = nil
    var name: String? = nil
    var qth: String? = nil
    var state: String? = nil
    var country: String? = nil
    var qsoKind: String? = nil
    var contestExchange: String? = nil
    var satelliteName: String? = nil
    var potaReferences: String? = nil
    var sotaReferences: String? = nil
    var notes: String? = nil
    var source: String? = nil
}

struct DeleteQSOBridgeRequest: Codable {
    var appSupportDir: String
    var qsoId: String
    var operationId: String
    var deviceId: String?
}

struct QSOBridgeMutationResult: Decodable {
    var accepted: Bool
    var idempotent: Bool
    var officialEvent: RustOfficialEvent
    var qso: RustQSORecord?
    var projection: RustProjectionStatus?
    var sync: RustSyncMutationStatus?
}

struct RustOfficialEvent: Decodable {
    var eventId: String
    var eventType: String
    var entityId: String?
    var eventHash: String
    var correlationId: String
    var schemaVersion: Int
    var timestamp: String
}

struct RustQSORecord: Decodable {
    var qsoId: String
    var payload: RustQSOPayload
    var deleted: Bool
    var lastEventHash: String
    var projectionSource: String?
    var schemaVersion: Int?
}

struct RustQSOPayload: Decodable {
    var contactedCallsign: String?
    var stationCallsign: String?
    var operatorCallsign: String?
    var startedAt: String?
    var mode: String?
    var band: String?
    var submode: String?
    var frequencyHz: UInt64?
    var frequencyMhz: Double?
    var rstSent: String?
    var rstReceived: String?
    var powerWatts: Double?
    var stationProfileId: String?
    var equipmentSummary: String?
    var grid: String?
    var county: String?
    var name: String?
    var qth: String?
    var state: String?
    var country: String?
    var qsoKind: String?
    var contestExchange: String?
    var satelliteName: String?
    var potaReferences: String?
    var sotaReferences: String?
    var notes: String?
    var clientOperationId: String?
}

struct RustProjectionStatus: Decodable {
    var source: String?
    var schemaVersion: Int?
    var lastRustRevision: String?
    var pendingEventCount: Int?
}

struct RustSyncMutationStatus: Decodable {
    var pendingEventCount: Int?
    var authority: String?
}

struct StationProfileMutationRequest: Codable {
    var appSupportDir: String
    var stationProfileId: String
    var displayName: String
    var stationCallsign: String
    var operatorCallsign: String?
    var profileType: String?
    var defaultGrid: String?
    var defaultQth: String?
    var defaultPowerWatts: Int?
    var notes: String?
    var active: Bool?
}

struct StationEquipmentMutationRequest: Codable {
    var appSupportDir: String
    var equipmentId: String
    var equipmentType: String
    var displayName: String
    var manufacturer: String?
    var model: String?
    var serialNumber: String?
    var capabilities: [String]
    var notes: String?
}

struct SelectStationProfileBridgeRequest: Codable {
    var appSupportDir: String
    var stationProfileId: String
}

struct StationBookMutationResult: Decodable {
    var profile: StationProfileSnapshot?
    var equipment: EquipmentSnapshot?
    var stationBook: StationBookSnapshot
    var idempotent: Bool?
    var projectionSource: String?
}

struct BridgeSelfTestResult: Decodable {
    var success: Bool
    var libraryLinked: Bool
    var abiVersion: Int
    var bridgeSchemaVersion: Int
    var coreVersion: String
    var syncProtocolVersion: Int?
    var backupSchemaVersion: Int?
    var jsonRoundTrip: Bool?
    var errorRoundTrip: Bool?
    var allocationModel: String?
}

struct DomainMutationResult: Decodable {
    var accepted: Bool
    var officialEvent: RustOfficialEvent
    var projection: RustProjectionStatus?
}

struct ApplicationSettingsBridgeResult: Decodable {
    var exists: Bool
    var created: Bool
    var settings: RustApplicationSettings?
    var recordCount: Int
}

struct ApplicationSettingsUpdateBridgeRequest: Encodable {
    var appSupportDir: String
    var settings: RustApplicationSettings
}

struct RustApplicationSettings: Codable, Equatable {
    var schemaVersion: Int
    var `operator`: RustOperatorIdentitySettings
    var location: RustLocationSettings
    var providers: RustProviderSettings
    var sync: RustSyncSettings
    var logging: RustLoggingSettings
    var activation: RustActivationSettings
    var netControl: RustNetControlSettings
    var display: RustDisplaySettings
    var backup: RustBackupSettings
    var privacy: RustPrivacySettings
    var diagnostics: RustDiagnosticsSettings
    var developer: RustDeveloperSettings
    var createdAt: String
    var updatedAt: String
}

struct RustOperatorIdentitySettings: Codable, Equatable {
    var primaryCallsign: String
    var additionalCallsigns: [String]
    var operatorName: String?
    var operatorEmail: String?
    var stationCallsign: String
    var defaultStationProfileId: String?
    var defaultEquipmentProfileId: String?
}

struct RustLocationSettings: Codable, Equatable {
    var useDeviceLocation: Bool
    var manualGridOverrideEnabled: Bool
    var manualMaidenheadGrid: String?
    var lastGpsGrid: String?
    var lastLocationSource: String?
    var manualLocationName: String?
    var manualCounty: String?
    var manualState: String?
    var manualCountry: String?
}

struct RustProviderSettings: Codable, Equatable {
    var enabled: [String: Bool]
    var credentialMetadata: [String: [String: String]]
    var validation: [String: RustProviderValidationSettings]
}

struct RustProviderValidationSettings: Codable, Equatable {
    var configured: Bool
    var validated: Bool
    var validatedAt: String?
    var message: String
}

struct RustSyncSettings: Codable, Equatable {
    var syncServerUrl: String
    var deviceName: String
    var preferLanSync: Bool
    var autoPushEnabled: Bool
    var autoPullEnabled: Bool
    var syncIntervalMinutes: Int
    var backgroundSyncEnabled: Bool
    var accountLabel: String?
}

struct RustLoggingSettings: Codable, Equatable {
    var defaultBand: String
    var defaultMode: String
    var autoUppercaseCallsigns: Bool
    var askForLocationLater: Bool
    var callsignLookupPreference: String
}

struct RustActivationSettings: Codable, Equatable {
    var allowOfflineActivations: Bool
    var validationTtlHours: Int
    var notesTemplate: String?
    var potaUploadEnabled: Bool
    var sotaUploadEnabled: Bool
}

struct RustNetControlSettings: Codable, Equatable {
    var defaultName: String?
    var defaultFrequencyMhz: String?
    var defaultMode: String
    var sortRosterByTrafficPriority: Bool
}

struct RustDisplaySettings: Codable, Equatable {
    var appearance: String
    var accentColorName: String
    var mapDefaultLayer: String
    var showQsoMapObjects: Bool
    var showStationMapMarkers: Bool
}

struct RustBackupSettings: Codable, Equatable {
    var includeDiagnosticsByDefault: Bool
}

struct RustPrivacySettings: Codable, Equatable {
    var providerNotificationsEnabled: Bool
}

struct RustDiagnosticsSettings: Codable, Equatable {
    var shareDiagnosticsWithLogs: Bool
}

struct RustDeveloperSettings: Codable, Equatable {
    var developerModeEnabled: Bool
}

struct ActivationBridgeRequest: Codable {
    var appSupportDir: String
    var activationType: String
    var stationCallsign: String
    var operatorCallsign: String
    var startedAt: String
    var parkId: String?
    var summitId: String?
    var grid: String?
    var locationName: String?
    var notes: String?
}

struct ActivationEndBridgeRequest: Codable {
    var appSupportDir: String
    var activationId: String
    var endedAt: String
}

struct NetSessionStartBridgeRequest: Codable {
    var appSupportDir: String
    var netName: String
    var stationCallsign: String
    var netControlOperatorId: String
    var startedAt: String
    var frequencyHz: UInt64?
    var band: String?
    var mode: String?
    var notes: String?
}

struct NetSessionEndBridgeRequest: Codable {
    var appSupportDir: String
    var netSessionId: String
    var endedAt: String
}

struct NetCheckInBridgeRequest: Codable {
    var appSupportDir: String
    var netSessionId: String
    var callsign: String?
    var operatorName: String?
    var location: String?
    var grid: String?
    var tacticalCallsign: String?
    var status: String?
    var traffic: String?
    var checkinTime: String
    var late: Bool?
    var emergencyTraffic: Bool?
}

struct NetTrafficBridgeRequest: Codable {
    var appSupportDir: String
    var netSessionId: String
    var fromCallsign: String?
    var toCallsign: String?
    var precedence: String
    var summary: String
}

enum RustBridgePaths {
    static func applicationSupportDirectory() throws -> URL {
        guard let url = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            throw RustBridgeError.unavailable("Application Support directory is unavailable.")
        }
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}
