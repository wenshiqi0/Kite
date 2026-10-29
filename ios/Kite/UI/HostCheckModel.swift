import Foundation
import Observation

struct HerdrConnectionViewData: Equatable {
    let herdrPath: String
    let capability: String
    let version: String?
    let hostKeyFingerprint: String
}
enum CheckState: Equatable {
    case idle
    case running
    case passed(HerdrConnectionViewData)
    case confirmHostKey(HostKeyConfirmation)
    case failed(message: String, hostKeyChanged: Bool)
}

@MainActor
@Observable
final class HostCheckModel {
    private let engine = CoreController.shared
    var form = HostForm()
    private var localFailure: String?
    var deviceKey: DeviceKeyInfo? { engine.snapshot?.deviceKey }
    var deviceKeyError: String? { deviceKey == nil ? engine.nativeError ?? engine.snapshot?.failure?.details : nil }
    var isRunning: Bool { engine.snapshot?.connection == .connecting }
    var isConnected: Bool { engine.snapshot?.connection == .connected }
    var pendingHostKey: HostKeyConfirmation? { engine.snapshot?.pendingHostKey }
    var failureDetails: String? { engine.snapshot?.failure?.details ?? engine.nativeError }
    var state: CheckState {
        if let localFailure { return .failed(message: localFailure, hostKeyChanged: false) }
        guard let snapshot = engine.snapshot else { return .idle }
        if let pending = snapshot.pendingHostKey { return .confirmHostKey(pending) }
        if snapshot.connection == .connecting { return .running }
        if let failure = snapshot.failure {
            let message: String
            switch failure.kind {
            case .authentication: message = localized("error_auth_rejected", [failure.username])
            case .hostKeyChanged: message = localized("error_host_key_changed", [failure.trustedFingerprint, failure.presentedFingerprint])
            case .herdrMissing: message = localized("error_herdr_not_found")
            default: message = localized("error_action")
            }
            return .failed(message: message, hostKeyChanged: failure.kind == .hostKeyChanged)
        }
        if snapshot.connection == .connected {
            return .passed(HerdrConnectionViewData(herdrPath: snapshot.herdrPath ?? "", capability: snapshot.capability ?? "",
                version: snapshot.herdrVersion, hostKeyFingerprint: snapshot.hostKeyFingerprint ?? ""))
        }
        return .idle
    }
    func loadDeviceKey() {
        Task {
            do {
                try await engine.initialize()
                if let snapshot = engine.snapshot,
                   let h = snapshot.hosts.first(where: { $0.id == snapshot.selectedHost }) ?? snapshot.hosts.first {
                    form = HostForm(id: h.id, host: h.host, port: String(h.port), username: h.username, herdrPath: h.herdrPath ?? "")
                }
            } catch { engine.nativeError = String(describing: error) }
        }
    }
    func check() {
        if isConnected { engine.command { try await $0.disconnect() }; return }
        guard !isRunning else { return }
        let f = form
        guard !f.host.trimmingCharacters(in: .whitespaces).isEmpty else { localFailure = localized("error_host_required"); return }
        guard !f.username.trimmingCharacters(in: .whitespaces).isEmpty else { localFailure = localized("error_username_required"); return }
        guard let port = UInt16(f.port.trimmingCharacters(in: .whitespaces)), port > 0 else { localFailure = localized("error_port_invalid"); return }
        localFailure = nil
        engine.command { [weak self] core in
            let id = try await core.saveHost(host: HostRecord(id: f.id, host: f.host, port: port, username: f.username,
                herdrPath: f.herdrPath.isEmpty ? nil : f.herdrPath, streamHost: nil, sunshinePort: 47989))
            self?.form.id = id
            try await core.connect(hostId: id)
        }
    }
    func trustHostKey(_ c: HostKeyConfirmation) { engine.command { try await $0.trustHost(hostId: c.hostId, fingerprint: c.fingerprint) } }
    func dismissHostKey() { engine.command { try await $0.disconnect() } }
    func forgetHostKey() { guard let id = engine.snapshot?.selectedHost else { return }; engine.command { try await $0.forgetHost(hostId: id) } }
    private func localized(_ key: String, _ args: [CVarArg] = []) -> String { String(format: NSLocalizedString(key, comment: ""), arguments: args) }
}
