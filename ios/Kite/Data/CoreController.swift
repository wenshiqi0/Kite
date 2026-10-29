import Foundation
import Observation
import UIKit

private final class StateObserver: CoreObserver, @unchecked Sendable {
    func stateChanged(snapshot: CoreSnapshot) {
        Task { @MainActor in CoreController.shared.accept(snapshot) }
    }
}

@MainActor
@Observable
final class CoreController {
    static let shared = CoreController()
    private(set) var snapshot: CoreSnapshot?
    var nativeError: String?
    private var core: KiteCore?
    private var initialization: Task<Void, Error>?
    private init() {
        do { core = try KiteCore(secretStore: DeviceKeyStore(), observer: StateObserver()) }
        catch { nativeError = String(describing: error) }
    }
    func accept(_ next: CoreSnapshot) {
        guard snapshot == nil || next.revision > snapshot!.revision else { return }
        snapshot = next
    }
    func initialize() async throws {
        if snapshot?.initialized == true { return }
        if let initialization { try await initialization.value; return }
        guard let core else { throw SecretStoreError.Failure(details: "core unavailable") }
        let folder = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: false).appendingPathComponent("core")
        let legacy = try HostStore().legacyData()
        let device = "kite@\(UIDevice.current.model)"
        let task = Task { try await core.initialize(dataDirectory: folder.path, deviceName: device, legacy: legacy) }
        initialization = task
        defer { initialization = nil }
        try await task.value
        accept(try core.snapshot())
    }
    func command(_ action: @escaping @MainActor (KiteCore) async throws -> Void) {
        guard let core else { return }
        Task { do { try await action(core) } catch { nativeError = String(describing: error) } }
    }
}
