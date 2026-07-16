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
