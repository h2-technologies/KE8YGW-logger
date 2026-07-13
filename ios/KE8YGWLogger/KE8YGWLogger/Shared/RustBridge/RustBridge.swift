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
        await assign(endpoint: .sync, to: \.sync, as: SyncSnapshot.self)
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
                "core_version": "0.1.0",
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
        case "bridge.self_test":
            data = [
                "success": true,
                "library_linked": false,
                "abi_version": 1,
                "bridge_schema_version": 1,
                "core_version": "0.1.0",
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
        "offline_queue": [],
        "conflicts": [],
        "history": []
    ]

    static let diagnostics: [String: Any] = [
        "rust_version": "0.1.0",
        "core_version": "0.1.0",
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
    var offlineQueue: [String]?
    var conflicts: [String]?
    var history: [String]?

    static let placeholder = SyncSnapshot(
        cloudConnectionState: "disconnected",
        pendingChanges: 0,
        offlineQueue: [],
        conflicts: [],
        history: []
    )
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
