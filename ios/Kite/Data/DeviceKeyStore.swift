import Foundation
import Security
import UIKit

/// This device's SSH key. The private key lives in the Keychain, readable only
/// on this device after first unlock and never synced to iCloud.
struct DeviceKeyStore {
    private let service = "io.github.wenshiqi0.kite"
    private let account = "device-ed25519"

    func loadOrCreate() throws -> SshKeyPair {
        if let stored = try read() {
            return try describeSshKey(privateKeyOpenssh: stored)
        }
        let pair = try generateSshKey(comment: "kite@\(UIDevice.current.model)")
        try store(pair.privateKeyOpenssh)
        return pair
    }

    private var baseQuery: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func read() throws -> String? {
        var query = baseQuery
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard let data = item as? Data else { throw KeychainError(status: errSecDecode) }
            return String(decoding: data, as: UTF8.self)
        case errSecItemNotFound:
            return nil
        default:
            throw KeychainError(status: status)
        }
    }

    private func store(_ privateKey: String) throws {
        var query = baseQuery
        query[kSecValueData as String] = Data(privateKey.utf8)
        query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }
}

struct KeychainError: Error, CustomStringConvertible {
    let status: OSStatus

    var description: String {
        let message = SecCopyErrorMessageString(status, nil) as String?
        return message ?? "OSStatus \(status)"
    }
}
