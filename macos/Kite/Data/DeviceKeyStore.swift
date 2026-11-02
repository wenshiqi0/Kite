import Foundation
import Security
import CryptoKit

/// The immutable component owns Keychain access across app updates.
final class DeviceKeyStore: SecretStore, @unchecked Sendable {
    private let helper = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/io.github.wenshiqi0.kite.mac/native/KiteKeychainHelper-v1")
    private func failure(_ text: String) -> SecretStoreError { .Failure(details: text) }
    private func validateHelper() throws {
        var me: SecCode?, own: SecStaticCode?, info: CFDictionary?
        guard SecCodeCopySelf([], &me) == 0, let me, SecCodeCopyStaticCode(me, [], &own) == 0, let own,
              SecCodeCopySigningInformation(own, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == 0,
              let values = info as? [String: Any],
              let certificates = values[kSecCodeInfoCertificates as String] as? [SecCertificate],
              let leaf = certificates.first else { throw failure("A persistent certificate signature is required") }
        let hash = Insecure.SHA1.hash(data: SecCertificateCopyData(leaf) as Data).map { String(format: "%02x", $0) }.joined()
        let text = "identifier \"io.github.wenshiqi0.kite.mac.keychain-helper\" and certificate leaf = H\"\(hash)\""
        var requirement: SecRequirement?, code: SecStaticCode?
        guard SecRequirementCreateWithString(text as CFString, [], &requirement) == 0, let requirement,
              SecStaticCodeCreateWithPath(helper as CFURL, [], &code) == 0, let code,
              SecStaticCodeCheckValidity(code, [], requirement) == 0 else {
            throw failure("Run macos/scripts/install-keychain-helper.sh once for this signing identity")
        }
    }
    private func invoke(_ operation: String, name: String, value: Data? = nil) throws -> Data? {
        guard ["ssh-ed25519", "sunshine-identity"].contains(name), (value?.count ?? 0) <= 64 * 1024 else {
            throw failure("Unsupported secret operation")
        }
        try validateHelper()
        let task = Process(), input = Pipe(), output = Pipe(), errors = Pipe()
        task.executableURL = helper; task.arguments = [operation, name]
        task.standardInput = input; task.standardOutput = output; task.standardError = errors
        try task.run()
        let deadline = DispatchWorkItem { if task.isRunning { task.terminate() } }
        DispatchQueue.global().asyncAfter(deadline: .now() + 12, execute: deadline)
        defer { deadline.cancel() }
        if let value { try input.fileHandleForWriting.write(contentsOf: value) }
        try input.fileHandleForWriting.close()
        let bytes = output.fileHandleForReading.readDataToEndOfFile()
        let diagnostics = errors.fileHandleForReading.readDataToEndOfFile()
        task.waitUntilExit()
        if operation == "read", task.terminationStatus == 44 { return nil }
        guard task.terminationStatus == 0, bytes.count <= 64 * 1024 else {
            throw failure(String(data: diagnostics.prefix(1024), encoding: .utf8) ?? "Keychain component unavailable")
        }
        return bytes
    }
    func read(name: String) throws -> Data? { try invoke("read", name: name) }
    func write(name: String, value: Data) throws { _ = try invoke("write", name: name, value: value) }
}
