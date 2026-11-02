import Foundation
import Security

/// A new, Mac-local identity. Never reads another device's key or SSH config.
final class DeviceKeyStore: SecretStore, @unchecked Sendable {
    private func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: "io.github.wenshiqi0.kite.mac",
         kSecAttrAccount as String: name,
         kSecAttrSynchronizable as String: false]
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
        let status = SecItemUpdate(q as CFDictionary, [kSecValueData as String: value] as CFDictionary)
        if status == errSecSuccess { return }
        guard status == errSecItemNotFound else { throw SecretStoreError.Failure(details: "Keychain update OSStatus \(status)") }
        var add = q
        add[kSecValueData as String] = value
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let result = SecItemAdd(add as CFDictionary, nil)
        guard result == errSecSuccess else { throw SecretStoreError.Failure(details: "Keychain write OSStatus \(result)") }
    }
}
