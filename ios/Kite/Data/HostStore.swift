import Foundation

/// What the user typed into the host form, kept as text until it is checked.
struct HostForm: Equatable {
    var host = ""
    var port = "22"
    var username = ""
    var herdrPath = ""

    /// Identifies the host for host-key trust.
    var address: String {
        "\(host.trimmingCharacters(in: .whitespaces).lowercased()):\(port.trimmingCharacters(in: .whitespaces))"
    }
}

/// The last host the user checked, and the host keys they chose to trust.
struct HostStore {
    private let defaults = UserDefaults.standard

    func loadForm() -> HostForm {
        HostForm(
            host: defaults.string(forKey: "host") ?? "",
            port: defaults.string(forKey: "port") ?? "22",
            username: defaults.string(forKey: "username") ?? "",
            herdrPath: defaults.string(forKey: "herdr_path") ?? ""
        )
    }

    func saveForm(_ form: HostForm) {
        defaults.set(form.host, forKey: "host")
        defaults.set(form.port, forKey: "port")
        defaults.set(form.username, forKey: "username")
        defaults.set(form.herdrPath, forKey: "herdr_path")
    }

    func trustedHostKey(for address: String) -> String? {
        defaults.string(forKey: hostKeyEntry(address))
    }

    func trustHostKey(_ hostKey: String, for address: String) {
        defaults.set(hostKey, forKey: hostKeyEntry(address))
    }

    func forgetHostKey(for address: String) {
        defaults.removeObject(forKey: hostKeyEntry(address))
    }

    private func hostKeyEntry(_ address: String) -> String {
        "host_key:\(address)"
    }
}
