import Foundation
import CoreLocation
import Network
import UserNotifications

#if os(iOS)
import Security
#endif

struct StoredCredential: Identifiable, Hashable {
    var id: String
    var label: String
    var providerId: String
    var updatedAt: Date
}

protocol CredentialVault {
    func save(secret: String, account: String, providerId: String) throws
    func read(account: String, providerId: String) throws -> String?
    func delete(account: String, providerId: String) throws
}

enum CredentialVaultError: LocalizedError {
    case unhandledStatus(Int32)

    var errorDescription: String? {
        switch self {
        case .unhandledStatus(let status):
            return "Keychain operation failed with status \(status)."
        }
    }
}

struct KeychainCredentialVault: CredentialVault {
    private let service = "com.h2technologiesllc.ke8ygw-logger.credentials"

    func save(secret: String, account: String, providerId: String) throws {
        #if os(iOS)
        let key = key(account: account, providerId: providerId)
        let data = Data(secret.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key
        ]
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]
        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            insert[kSecValueData as String] = data
            insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let insertStatus = SecItemAdd(insert as CFDictionary, nil)
            guard insertStatus == errSecSuccess else {
                throw CredentialVaultError.unhandledStatus(insertStatus)
            }
        } else if status != errSecSuccess {
            throw CredentialVaultError.unhandledStatus(status)
        }
        #else
        _ = (secret, account, providerId)
        #endif
    }

    func read(account: String, providerId: String) throws -> String? {
        #if os(iOS)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key(account: account, providerId: providerId),
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            throw CredentialVaultError.unhandledStatus(status)
        }
        guard let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
        #else
        _ = (account, providerId)
        return nil
        #endif
    }

    func delete(account: String, providerId: String) throws {
        #if os(iOS)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key(account: account, providerId: providerId)
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw CredentialVaultError.unhandledStatus(status)
        }
        #else
        _ = (account, providerId)
        #endif
    }

    private func key(account: String, providerId: String) -> String {
        "\(providerId):\(account)"
    }
}

enum LocationPermissionState: String {
    case notDetermined
    case allowedWhenInUse
    case allowedAlways
    case denied
    case restricted
    case unknown

    var label: String {
        switch self {
        case .notDetermined: return "Not Requested"
        case .allowedWhenInUse: return "Allowed While Using"
        case .allowedAlways: return "Allowed"
        case .denied: return "Denied"
        case .restricted: return "Restricted"
        case .unknown: return "Unknown"
        }
    }

    static func from(_ status: CLAuthorizationStatus) -> LocationPermissionState {
        switch status {
        case .notDetermined: return .notDetermined
        case .authorizedWhenInUse: return .allowedWhenInUse
        case .authorizedAlways: return .allowedAlways
        case .denied: return .denied
        case .restricted: return .restricted
        @unknown default: return .unknown
        }
    }
}

final class LocationCoordinator: NSObject, ObservableObject, CLLocationManagerDelegate {
    @Published var permissionState: LocationPermissionState = .unknown
    @Published var currentGrid: String?
    @Published var statusMessage = "Location has not been requested."
    @Published var locationSource: MaidenheadLocationSource = .unknown

    private let manager = CLLocationManager()

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
        permissionState = LocationPermissionState.from(manager.authorizationStatus)
    }

    func requestCurrentGrid(useDeviceLocation: Bool) {
        guard useDeviceLocation else {
            manager.stopUpdatingLocation()
            locationSource = .unknown
            statusMessage = "Device location use is disabled in app settings."
            return
        }

        let status = manager.authorizationStatus
        permissionState = LocationPermissionState.from(status)
        switch status {
        case .notDetermined:
            statusMessage = "Requesting location permission."
            manager.requestWhenInUseAuthorization()
        case .authorizedWhenInUse, .authorizedAlways:
            statusMessage = "Requesting current location."
            manager.requestLocation()
        case .denied:
            locationSource = .unknown
            statusMessage = "Location permission is denied. Change it in iOS Settings or use a manual grid."
        case .restricted:
            locationSource = .unknown
            statusMessage = "Location permission is restricted. Use a manual grid."
        @unknown default:
            locationSource = .unknown
            statusMessage = "Location permission status is unknown."
        }
    }

    func stopUsingLocation() {
        manager.stopUpdatingLocation()
        locationSource = .unknown
        statusMessage = "Device location use is disabled in app settings."
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        let status = manager.authorizationStatus
        DispatchQueue.main.async {
            self.permissionState = LocationPermissionState.from(status)
            if status == .authorizedWhenInUse || status == .authorizedAlways {
                manager.requestLocation()
            } else if status == .denied {
                self.statusMessage = "Location permission is denied. Manual grid entry remains available."
            } else if status == .restricted {
                self.statusMessage = "Location permission is restricted. Manual grid entry remains available."
            }
        }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else { return }
        let coordinate = location.coordinate
        let grid = HamRadioUtilities.maidenheadGrid(latitude: coordinate.latitude, longitude: coordinate.longitude)
        DispatchQueue.main.async {
            self.currentGrid = grid
            self.locationSource = grid == nil ? .unknown : .gps
            self.statusMessage = grid.map { "Current GPS grid: \($0)" } ?? "Could not calculate a Maidenhead grid."
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        DispatchQueue.main.async {
            self.locationSource = .unknown
            self.statusMessage = "Location unavailable: \(error.localizedDescription)"
        }
    }
}

enum ConnectivityState: String {
    case online
    case offline
    case requiresConnection
    case unknown

    var label: String {
        switch self {
        case .online: return "Online"
        case .offline: return "Offline"
        case .requiresConnection: return "Requires Connection"
        case .unknown: return "Unknown"
        }
    }

    var hasUsableInternet: Bool {
        self == .online
    }
}

struct SyncLanDiscoveryConfiguration: Equatable {
    var protocolName = "ke8ygw-logger-sync"
    var protocolVersion = 1
    var ipv4MulticastHost = "239.73.89.71"
    var ipv6MulticastHost = "ff12::73:5947"
    var discoveryPort: UInt16 = 9737
    var peerTimeoutSeconds: TimeInterval = 45

    static let `default` = SyncLanDiscoveryConfiguration()

    var multicastEndpoints: [NWEndpoint] {
        let port = NWEndpoint.Port(rawValue: discoveryPort) ?? NWEndpoint.Port(rawValue: 9737)!
        return [
            .hostPort(host: NWEndpoint.Host(ipv4MulticastHost), port: port),
            .hostPort(host: NWEndpoint.Host(ipv6MulticastHost), port: port)
        ]
    }
}

struct SyncLanDiscoveryPacket: Codable, Equatable {
    var protocolName: String
    var protocolVersion: Int
    var deviceId: String
    var sessionId: String
    var userHash: String?
    var displayName: String
    var capabilities: [String]
    var localApiPort: UInt16?
    var timestamp: String

    enum CodingKeys: String, CodingKey {
        case protocolName = "protocol_name"
        case protocolVersion = "protocol_version"
        case deviceId = "device_id"
        case sessionId = "session_id"
        case userHash = "user_hash"
        case displayName = "display_name"
        case capabilities
        case localApiPort = "local_api_port"
        case timestamp
    }
}

struct SyncLanDiscoveredPeer: Identifiable, Equatable {
    var id: String { "\(deviceId):\(sessionId)" }
    var deviceId: String
    var sessionId: String
    var displayName: String
    var peerURL: URL
    var sourceAddress: String
    var capabilities: [String]
    var firstSeen: Date
    var lastSeen: Date

    var detailLabel: String {
        "\(peerURL.absoluteString) / \(deviceId)"
    }
}

enum SyncLanDiscoveryScannerError: LocalizedError, Equatable {
    case missingLocalIdentity
    case noUsableMulticastGroup

    var errorDescription: String? {
        switch self {
        case .missingLocalIdentity:
            return "The local LAN sync identity is unavailable."
        case .noUsableMulticastGroup:
            return "LAN discovery could not join an IPv4 or IPv6 multicast group."
        }
    }
}

@MainActor
final class SyncLanDiscoveryScanner: ObservableObject {
    @Published private(set) var isRunning = false
    @Published private(set) var discoveredPeers: [SyncLanDiscoveredPeer] = []
    @Published var lastError: String?

    private let config: SyncLanDiscoveryConfiguration
    private let queue = DispatchQueue(label: "KE8YGWLogger.SyncLanDiscovery")
    private var groups: [NWConnectionGroup] = []
    private var localIdentity: SyncPeerIdentity?

    init(config: SyncLanDiscoveryConfiguration = .default) {
        self.config = config
    }

    func start(identity: SyncPeerIdentity?) {
        stop()
        guard let identity else {
            lastError = SyncLanDiscoveryScannerError.missingLocalIdentity.localizedDescription
            return
        }
        localIdentity = identity
        lastError = nil

        for endpoint in config.multicastEndpoints {
            do {
                let group = try makeGroup(endpoint: endpoint)
                groups.append(group)
                group.start(queue: queue)
            } catch {
                lastError = error.localizedDescription
            }
        }

        isRunning = !groups.isEmpty
        if groups.isEmpty {
            lastError = SyncLanDiscoveryScannerError.noUsableMulticastGroup.localizedDescription
        }
    }

    func stop() {
        groups.forEach { $0.cancel() }
        groups.removeAll()
        isRunning = false
    }

    func usePeer(_ peer: SyncLanDiscoveredPeer) -> (url: String, deviceId: String, displayName: String) {
        (peer.peerURL.absoluteString, peer.deviceId, peer.displayName)
    }

    private func makeGroup(endpoint: NWEndpoint) throws -> NWConnectionGroup {
        let multicastGroup = try NWMulticastGroup(for: [endpoint])
        let parameters = NWParameters.udp
        parameters.allowLocalEndpointReuse = true
        let group = NWConnectionGroup(with: multicastGroup, using: parameters)
        group.stateUpdateHandler = { [weak self] state in
            if case .failed(let error) = state {
                Task { @MainActor in
                    self?.lastError = error.localizedDescription
                }
            }
        }
        group.setReceiveHandler(maximumMessageSize: 4096, rejectOversizedMessages: true) { [weak self] message, content, _ in
            guard let self,
                  let content,
                  let remoteEndpoint = message.remoteEndpoint
            else {
                return
            }
            Task {
                await self.processDiscoveryDatagram(content, remoteEndpoint: remoteEndpoint)
            }
        }
        return group
    }

    private func processDiscoveryDatagram(_ data: Data, remoteEndpoint: NWEndpoint) async {
        guard let identity = localIdentity,
              let packet = Self.decodeDiscoveryPacket(data),
              Self.isSupported(packet, config: config),
              !Self.isSelf(packet, identity: identity),
              let peerURL = Self.peerURL(packet: packet, remoteEndpoint: remoteEndpoint)
        else {
            return
        }

        do {
            let state = try await Self.fetchPeerState(peerURL: peerURL)
            guard Self.peerStateMatches(packet: packet, state: state) else {
                return
            }
            upsertPeer(packet: packet, peerURL: peerURL, remoteEndpoint: remoteEndpoint)
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func upsertPeer(packet: SyncLanDiscoveryPacket, peerURL: URL, remoteEndpoint: NWEndpoint) {
        pruneExpiredPeers()
        let now = Date()
        let sourceAddress = Self.sourceAddress(remoteEndpoint) ?? peerURL.absoluteString
        let displayName = packet.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        let peer = SyncLanDiscoveredPeer(
            deviceId: packet.deviceId,
            sessionId: packet.sessionId,
            displayName: displayName.isEmpty ? "LAN Peer" : displayName,
            peerURL: peerURL,
            sourceAddress: sourceAddress,
            capabilities: packet.capabilities,
            firstSeen: discoveredPeers.first(where: { $0.deviceId == packet.deviceId && $0.sessionId == packet.sessionId })?.firstSeen ?? now,
            lastSeen: now
        )
        if let index = discoveredPeers.firstIndex(where: { $0.id == peer.id }) {
            discoveredPeers[index] = peer
        } else {
            discoveredPeers.append(peer)
        }
        discoveredPeers.sort { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
    }

    private func pruneExpiredPeers(now: Date = Date()) {
        discoveredPeers.removeAll { now.timeIntervalSince($0.lastSeen) > config.peerTimeoutSeconds }
    }

    nonisolated static func decodeDiscoveryPacket(_ data: Data) -> SyncLanDiscoveryPacket? {
        try? JSONDecoder().decode(SyncLanDiscoveryPacket.self, from: data)
    }

    nonisolated static func isSupported(_ packet: SyncLanDiscoveryPacket, config: SyncLanDiscoveryConfiguration = .default) -> Bool {
        packet.protocolName == config.protocolName && packet.protocolVersion == config.protocolVersion
    }

    nonisolated static func isSelf(_ packet: SyncLanDiscoveryPacket, identity: SyncPeerIdentity) -> Bool {
        equivalentUUID(packet.deviceId, identity.deviceId) && equivalentUUID(packet.sessionId, identity.sessionId)
    }

    nonisolated static func peerStateMatches(packet: SyncLanDiscoveryPacket, state: SyncLanPeerStateResponse) -> Bool {
        guard let identity = state.identity else {
            return false
        }
        return equivalentUUID(packet.deviceId, identity.deviceId)
            && equivalentUUID(packet.sessionId, identity.sessionId)
    }

    nonisolated static func peerURL(packet: SyncLanDiscoveryPacket, remoteEndpoint: NWEndpoint) -> URL? {
        guard case .hostPort(let host, let sourcePort) = remoteEndpoint,
              let hostString = hostString(host),
              isUsableDiscoveryHost(hostString)
        else {
            return nil
        }
        let port = Int(packet.localApiPort ?? sourcePort.rawValue)
        guard port > 0 else {
            return nil
        }
        var components = URLComponents()
        components.scheme = "http"
        components.host = hostString
        components.port = port
        return components.url
    }

    nonisolated static func sourceAddress(_ endpoint: NWEndpoint) -> String? {
        guard case .hostPort(let host, let port) = endpoint,
              let hostString = hostString(host)
        else {
            return nil
        }
        return "\(hostString):\(port.rawValue)"
    }

    nonisolated private static func fetchPeerState(peerURL: URL) async throws -> SyncLanPeerStateResponse {
        let request = try SyncLanHTTPTransport().makeStateRequest(peerURL: peerURL)
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode)
        else {
            throw SyncLanHTTPTransportError.invalidHTTPResponse
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(SyncLanPeerStateResponse.self, from: data)
    }

    nonisolated private static func hostString(_ host: NWEndpoint.Host) -> String? {
        switch host {
        case .name(let name, _):
            return name
        case .ipv4(let address):
            return "\(address)"
        case .ipv6(let address):
            return "\(address)"
        @unknown default:
            return nil
        }
    }

    nonisolated private static func isUsableDiscoveryHost(_ host: String) -> Bool {
        let normalized = host.lowercased()
        return !normalized.hasPrefix("fe80:") || normalized.contains("%")
    }

    nonisolated private static func equivalentUUID(_ left: String, _ right: String) -> Bool {
        guard let left = UUID(uuidString: left),
              let right = UUID(uuidString: right)
        else {
            return false
        }
        return left.uuidString.lowercased() == right.uuidString.lowercased()
    }
}

@MainActor
final class ConnectivityMonitor: ObservableObject {
    @Published var state: ConnectivityState = .unknown

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(label: "KE8YGWLogger.ConnectivityMonitor")
    private var started = false

    func start() {
        guard !started else { return }
        started = true
        monitor.pathUpdateHandler = { [weak self] path in
            let state: ConnectivityState
            switch path.status {
            case .satisfied:
                state = .online
            case .requiresConnection:
                state = .requiresConnection
            case .unsatisfied:
                state = .offline
            @unknown default:
                state = .unknown
            }
            Task { @MainActor in
                self?.state = state
            }
        }
        monitor.start(queue: queue)
    }

    func stop() {
        guard started else { return }
        monitor.cancel()
        started = false
    }
}

@MainActor
final class NotificationCoordinator: ObservableObject {
    @Published var authorizationStatus: UNAuthorizationStatus = .notDetermined

    func refreshAuthorizationStatus() async {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        authorizationStatus = settings.authorizationStatus
    }

    func requestAuthorization() async {
        _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound])
        await refreshAuthorizationStatus()
    }

    func scheduleReminder(id: String, title: String, body: String, secondsFromNow: TimeInterval) async {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(secondsFromNow, 5), repeats: false)
        let request = UNNotificationRequest(identifier: id, content: content, trigger: trigger)
        try? await UNUserNotificationCenter.current().add(request)
    }
}
