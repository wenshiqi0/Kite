import Foundation
import Observation

/// First connection to a host: the user must confirm its fingerprint.
struct HostKeyConfirmation: Equatable {
    let address: String
    let fingerprint: String
    let hostKey: String
}

enum CheckState: Equatable {
    case idle
    case running
    case passed(HerdrCheck)
    case confirmHostKey(HostKeyConfirmation)
    case failed(message: String, hostKeyChanged: Bool)
}

@MainActor
@Observable
final class HostCheckModel {
    private let deviceKeys = DeviceKeyStore()
    private let hosts = HostStore()

    private(set) var deviceKey: SshKeyPair?
    private(set) var deviceKeyError: String?
    var form: HostForm
    private(set) var state: CheckState = .idle

    init() {
        form = hosts.loadForm()
    }

    var isRunning: Bool { state == .running }

    var pendingHostKey: HostKeyConfirmation? {
        if case let .confirmHostKey(confirmation) = state { confirmation } else { nil }
    }

    func loadDeviceKey() {
        guard deviceKey == nil else { return }
        do {
            deviceKey = try deviceKeys.loadOrCreate()
        } catch {
            deviceKeyError = String(describing: error)
        }
    }

    func check() {
        guard let key = deviceKey, !isRunning else { return }
        let form = form
        let host = form.host.trimmingCharacters(in: .whitespaces)
        let username = form.username.trimmingCharacters(in: .whitespaces)
        let herdrPath = form.herdrPath.trimmingCharacters(in: .whitespaces)
        guard !host.isEmpty else { return fail("error_host_required") }
        guard !username.isEmpty else { return fail("error_username_required") }
        guard let port = UInt16(form.port.trimmingCharacters(in: .whitespaces)), port > 0 else {
            return fail("error_port_invalid")
        }
        hosts.saveForm(form)
        let config = HostConfig(
            host: host,
            port: port,
            username: username,
            herdrPath: herdrPath.isEmpty ? nil : herdrPath
        )
        let trusted = hosts.trustedHostKey(for: form.address)
        state = .running
        Task {
            do {
                let check = try await checkHerdr(
                    host: config,
                    privateKeyOpenssh: key.privateKeyOpenssh,
                    trustedHostKey: trusted
                )
                state = .passed(check)
            } catch let error as KiteError {
                state = describe(error, address: form.address)
            } catch {
                state = .failed(message: String(describing: error), hostKeyChanged: false)
            }
        }
    }

    func trustHostKey(_ confirmation: HostKeyConfirmation) {
        hosts.trustHostKey(confirmation.hostKey, for: confirmation.address)
        state = .idle
        check()
    }

    func dismissHostKey() {
        state = .idle
    }

    func forgetHostKey() {
        hosts.forgetHostKey(for: form.address)
        state = .idle
    }

    private func describe(_ error: KiteError, address: String) -> CheckState {
        switch error {
        case let .UnknownHostKey(fingerprint, hostKey):
            return .confirmHostKey(HostKeyConfirmation(address: address, fingerprint: fingerprint, hostKey: hostKey))
        case let .HostKeyChanged(trusted, presented):
            return .failed(message: localized("error_host_key_changed", [trusted, presented]), hostKeyChanged: true)
        case let .AuthRejected(username):
            return failure("error_auth_rejected", username)
        case .HerdrNotFound:
            return failure("error_herdr_not_found")
        case let .HerdrUnsupported(path, _):
            return failure("error_herdr_unsupported", path)
        case let .Connection(message):
            return failure("error_connection", message)
        case let .InvalidKey(message):
            return failure("error_invalid_key", message)
        case let .Probe(message):
            return failure("error_probe", message)
        }
    }

    private func fail(_ key: String) {
        state = failure(key)
    }

    private func failure(_ key: String, _ args: CVarArg...) -> CheckState {
        .failed(message: localized(key, args), hostKeyChanged: false)
    }

    private func localized(_ key: String, _ args: [CVarArg]) -> String {
        String(format: NSLocalizedString(key, comment: ""), arguments: args)
    }
}
