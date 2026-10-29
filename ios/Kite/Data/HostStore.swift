import Foundation

struct HostForm: Equatable {
    var id = ""
    var host = ""
    var port = "22"
    var username = ""
    var herdrPath = ""
}

/// M1 is a read-only migration source. New writes go to the Rust store.
struct HostStore {
    private let defaults = UserDefaults.standard
    func loadForm() -> HostForm {
        HostForm(host: defaults.string(forKey: "host") ?? "", port: defaults.string(forKey: "port") ?? "22",
                 username: defaults.string(forKey: "username") ?? "", herdrPath: defaults.string(forKey: "herdr_path") ?? "")
    }
    func legacyData() throws -> LegacyData {
        let f = loadForm()
        var host: HostRecord?
        if !f.host.isEmpty {
            guard let port = UInt16(f.port), port > 0 else { throw SecretStoreError.Failure(details: "invalid legacy SSH port") }
            host = HostRecord(id: "legacy-m1", host: f.host, port: port, username: f.username,
                              herdrPath: f.herdrPath.isEmpty ? nil : f.herdrPath, streamHost: nil, sunshinePort: 47989)
        }
        let keys = try defaults.dictionaryRepresentation().filter { $0.key.hasPrefix("host_key:") }.map { key, value in
            guard let publicKey = value as? String else { throw SecretStoreError.Failure(details: "invalid legacy host key") }
            return TrustedHostRecord(address: String(key.dropFirst("host_key:".count)), publicKey: publicKey)
        }
        return LegacyData(source: "ios-m1", host: host, trustedKeys: keys)
    }
}
