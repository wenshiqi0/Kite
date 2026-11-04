import Foundation
import Security
import Darwin

private let service = "io.github.wenshiqi0.kite.mac"
private let names: Set<String> = ["ssh-ed25519", "sunshine-identity"]
private let limit = 64 * 1024
private func fail(_ text: String, _ code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((text + "\n").utf8)); exit(code)
}
private func query(_ name: String) -> [String: Any] {
    [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
     kSecAttrAccount as String: name, kSecAttrSynchronizable as String: false]
}
private func read(_ name: String) -> (OSStatus, Data?) {
    var q = query(name); q[kSecReturnData as String] = true
    var value: CFTypeRef?
    let status = SecItemCopyMatching(q as CFDictionary, &value)
    return (status, value as? Data)
}
private func authorizedParent() -> Bool {
    let parent = getppid()
    guard parent > 1 else { return false }
    var caller: SecCode?, requirement: SecRequirement?
    guard SecCodeCopyGuestWithAttributes(nil, [kSecGuestAttributePid as String: parent] as CFDictionary, [], &caller) == 0,
          let caller, SecRequirementCreateWithString(kiteCallerRequirement as CFString, [], &requirement) == 0,
          let requirement else { return false }
    return SecCodeCheckValidity(caller, [], requirement) == errSecSuccess
}

/// Explicit setup only. The OS records persistent trust for this immutable
/// component. No keychain ACL is rewritten by application code.
private func authorize() {
    var changed = 0
    for name in names.sorted() {
        SecKeychainSetUserInteractionAllowed(false)
        let existing = read(name)
        if existing.0 == errSecSuccess || existing.0 == errSecItemNotFound { continue }
        SecKeychainSetUserInteractionAllowed(true)
        let status = read(name).0
        SecKeychainSetUserInteractionAllowed(false)
        guard status == 0 else { fail("Kite item authorization OSStatus \(status)") }
        changed += 1
    }
    print("Kite identity authorization ready; updated \(changed) items, data preserved.")
}

@main
private struct KeychainHelper {
    static func main() {
        let args = Array(CommandLine.arguments.dropFirst())
        if args == ["--version"] { print("1"); return }
        if args == ["--authorize"] { authorize(); return }
        guard args.count == 2, names.contains(args[1]), ["read", "write"].contains(args[0]), authorizedParent() else {
            fail("Keychain component caller is not authorized", 77)
        }
        SecKeychainSetUserInteractionAllowed(false)
        if args[0] == "read" {
            let (status, data) = read(args[1])
            if status == errSecItemNotFound { exit(44) }
            guard status == 0, let data, data.count <= limit else { fail("Keychain read OSStatus \(status)") }
            FileHandle.standardOutput.write(data)
        } else {
            var data = Data()
            while let chunk = try? FileHandle.standardInput.read(upToCount: 8192), !chunk.isEmpty {
                data.append(chunk)
                guard data.count <= limit else { fail("Keychain value exceeds limit") }
            }
            let q = query(args[1])
            var status = SecItemUpdate(q as CFDictionary, [kSecValueData as String: data] as CFDictionary)
            if status == errSecItemNotFound {
                var add = q; add[kSecValueData as String] = data
                add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
                status = SecItemAdd(add as CFDictionary, nil)
            }
            guard status == 0 else { fail("Keychain write OSStatus \(status)") }
        }
    }
}
