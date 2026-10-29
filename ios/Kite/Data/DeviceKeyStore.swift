import Foundation
import Security

/// Native secret adapter. The M1 account is retained verbatim.
final class DeviceKeyStore: SecretStore, @unchecked Sendable {
    private let service = "io.github.wenshiqi0.kite"
    private func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: name == "ssh-ed25519" ? "device-ed25519" : "secret-\(name)"]
    }
    func read(name: String) throws -> Data? {
        var q = query(name)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else {
            throw SecretStoreError.Failure(details: "Keychain read OSStatus \(status)")
        }
        return data
    }
    func write(name: String, value: Data) throws {
        let q = query(name)
        let update = SecItemUpdate(q as CFDictionary, [kSecValueData as String: value] as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw SecretStoreError.Failure(details: "Keychain update OSStatus \(update)") }
        var add = q
        add[kSecValueData as String] = value
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw SecretStoreError.Failure(details: "Keychain write OSStatus \(status)") }
    }
}
