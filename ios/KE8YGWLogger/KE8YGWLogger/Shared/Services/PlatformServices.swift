import Foundation
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
