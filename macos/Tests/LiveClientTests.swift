import XCTest
import SwiftUI
@testable import Kite

private final class LiveObserver: CoreObserver, @unchecked Sendable {
    var receive: (@Sendable (CoreSnapshot) -> Void)?
    func stateChanged(snapshot: CoreSnapshot) { receive?(snapshot) }
}
private final class LiveSecrets: SecretStore, @unchecked Sendable {
    let root: URL
    init(root: URL) { self.root = root }
    private func file(_ name: String) throws -> URL {
        guard name.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "-" }) else { throw SecretStoreError.Failure(details: "Invalid QA secret name") }
        return root.appendingPathComponent(name)
    }
    func read(name: String) throws -> Data? {
        let url = try file(name)
        return FileManager.default.fileExists(atPath: url.path) ? try Data(contentsOf: url) : nil
    }
    func write(name: String, value: Data) throws {
        let url = try file(name)
        try value.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
}

/// Explicit opt-in integration. Use only the owned 1080p QA Sunshine display;
/// a real Codex turn runs in the caller's disposable project directory.
final class LiveClientTests: XCTestCase {
    struct Configuration: Decodable {
        let host: String
        let username: String
        let fingerprint: String
        let project: String
        let output: String
        let secrets: String
    }
    @MainActor
    func testLiveRemoteClient() async throws {
        guard let path = ProcessInfo.processInfo.environment["KITE_MAC_LIVE_CONFIG"],
              FileManager.default.fileExists(atPath: path) else { throw XCTSkip("Owned-host integration is opt-in") }
        let config = try JSONDecoder().decode(Configuration.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard config.project.hasSuffix("/.cache/kite-macos-client-20261102/project"), config.host.hasSuffix(".local") else {
            XCTFail("Requires an explicit owned LAN QA target"); return
        }
        let output = URL(fileURLWithPath: config.output, isDirectory: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let directory = output.appendingPathComponent("core")
        // A disposable test identity avoids asking Keychain to authorize the
        // test runner as the user's production Mac client.
        let model = ClientModel()
        let observer = LiveObserver()
        observer.receive = { [weak model] snapshot in Task { @MainActor in model?.accept(snapshot) } }
        let core = try KiteCore(secretStore: LiveSecrets(root: URL(fileURLWithPath: config.secrets)), observer: observer)
        let receiver = model.media; receiver.audio.muted = true; receiver.attach()
        var window: NSWindow?
        do {
            try await core.initialize(dataDirectory: directory.path, deviceName: "kite@macOS", legacy: LegacyData(source: "owned-mac-qa", host: nil, trustedKeys: []))
            let existing = try core.snapshot().hosts.first { $0.host == config.host && $0.username == config.username && $0.sunshinePort == 48200 }
            let id = try await core.saveHost(host: HostRecord(id: existing?.id ?? "", host: config.host, port: 22, username: config.username,
                herdrPath: nil, streamHost: config.host, sunshinePort: 48200))
            try await core.connect(hostId: id)
            var state = try await wait(core) { $0.pendingHostKey != nil || $0.connection == .connected || $0.connection == .failed }
            if let pending = state.pendingHostKey {
                XCTAssertEqual(pending.fingerprint, config.fingerprint)
                guard pending.fingerprint == config.fingerprint else { throw failure("Independent SSH fingerprint mismatch") }
                try await core.trustHost(hostId: id, fingerprint: pending.fingerprint)
            }
            state = try await wait(core) { $0.connection == .connected }
            print("live_stage=connected")
            XCTAssertFalse(state.providers.isEmpty)
            let saved = state.sessions.filter { $0.cwd == config.project && $0.provider == "codex" }.max(by: { $0.updatedMs < $1.updatedMs })
            if let saved {
                if saved.archived { try await core.resumeSession(hostId: id, sessionId: saved.id) }
                else { try await core.openSession(hostId: id, sessionId: saved.id) }
            } else {
                try await core.createSession(hostId: id, provider: "codex", cwd: config.project, title: "Kite Mac client QA")
            }
            print("live_stage=session_created")
            state = try await wait(core, seconds: 60) { state in
                state.composer.ready && state.agents.contains { $0.id == state.selectedAgent && !$0.launchPending }
            }
            print("live_stage=chat_ready")
            if saved != nil { state = try await wait(core) { $0.history.complete } }
            let agent = try XCTUnwrap(state.selectedAgent), session = try XCTUnwrap(state.selectedSession)
            if !core.conversationSince(context: state.conversationContext, revision: 0).edits.flatMap(\.items).contains(where: { $0.kind == .assistant && $0.text.contains("MAC_CLIENT_QA_OK") }) {
                try await core.promptAgent(hostId: id, agentId: agent, text: "仅回复 MAC_CLIENT_QA_OK，不调用工具。")
            }
            state = try await wait(core, seconds: 90) { state in
                core.conversationSince(context: state.conversationContext, revision: 0).edits.flatMap(\.items)
                    .contains { $0.kind == .assistant && $0.text.contains("MAC_CLIENT_QA_OK") }
            }
            let context = state.conversationContext
            print("live_stage=chat_received")
            let source = CGSize(width: 1920, height: 1080)
            // Exercise the actual SwiftUI panel's Starting -> first frame ->
            // Streaming lifecycle, not a manually pre-mounted Metal canvas.
            let surface = NSHostingView(rootView: DesktopPane(model: model, expanded: .constant(false)))
            let panel = NSWindow(contentRect: CGRect(x: 100, y: 100, width: 960, height: 540), styleMask: [.titled, .closable], backing: .buffered, defer: false)
            panel.isReleasedWhenClosed = false
            panel.title = "Kite Mac owned integration"; panel.contentView = surface
            panel.makeKeyAndOrderFront(nil); window = panel
            try await core.startDesktop(hostId: id, mediaTarget: receiver.handle)
            state = try await wait(core) { $0.desktop.status == .pairing || $0.desktop.status == .streaming || $0.desktop.status == .failed }
            if let pin = state.desktop.pin { try await core.approvePairing(hostId: id, pin: pin) }
            state = try await wait(core) { $0.desktop.status == .streaming && $0.desktop.renderedFrames >= 30 }
            XCTAssertEqual(state.desktop.width, 1920); XCTAssertEqual(state.desktop.height, 1080)
            let startFrames = state.desktop.renderedFrames
            try await Task.sleep(for: .seconds(3))
            state = try core.snapshot()
            let displayFPS = Double(state.desktop.renderedFrames - startFrames) / 3
            let displayedFrames = state.desktop.renderedFrames
            print("live_stage=video_presented fps=\(displayFPS)")
            // These events only target the disposable Xvfb fixture, never :10.
            try await core.setTakeover(hostId: id, enabled: true)
            receiver.input.enabled = true
            receiver.input.move(CGPoint(x: 960, y: 420), source: source)
            receiver.input.button(1, down: true)
            receiver.input.move(CGPoint(x: 1150, y: 500), source: source)
            receiver.input.button(1, down: false)
            receiver.input.button(3, down: true); receiver.input.button(3, down: false)
            receiver.input.scroll(horizontal: 0, vertical: 120); receiver.input.barrier()
            receiver.input.key(0x77, down: true, modifiers: 0); receiver.input.key(0x77, down: false, modifiers: 0)
            receiver.input.move(CGPoint(x: 600, y: 945), source: source)
            receiver.input.button(1, down: true); receiver.input.button(1, down: false)
            receiver.input.key(0xA2, down: true, modifiers: 2)
            receiver.input.key(0x41, down: true, modifiers: 2); receiver.input.key(0x41, down: false, modifiers: 2)
            receiver.input.key(0xA2, down: false, modifiers: 0)
            receiver.input.key(0x08, down: true, modifiers: 0); receiver.input.key(0x08, down: false, modifiers: 0)
            receiver.input.text("Kite 远程桌面 29"); receiver.input.barrier()
            try await Task.sleep(for: .seconds(1))
            receiver.input.enabled = false; receiver.input.cancel()
            try await core.setTakeover(hostId: id, enabled: false)
            try await core.stopDesktop(hostId: try core.snapshot().selectedHost ?? "")
            try await core.disconnect()
            try await core.connect(hostId: id)
            _ = try await wait(core) { $0.connection == .connected }
            try await core.openSession(hostId: id, sessionId: session)
            state = try await wait(core) { $0.conversationCount >= 2 && $0.history.complete }
            XCTAssertTrue(core.conversationSince(context: state.conversationContext, revision: 0).edits.flatMap(\.items).contains { $0.text.contains("MAC_CLIENT_QA_OK") })
            let result: [String: Any] = ["ssh": true, "codex_turn": true, "history_reconnect": true,
                "source_width": 1920, "source_height": 1080, "display_fps": displayFPS,
                "rendered_frames": displayedFrames, "session_id": session, "conversation_context": context,
                "input": "sent to owned display; verify remote input.json separately", "audio": "muted; offline decode verified separately"]
            try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent("result.json"))
            try await core.archiveSession(hostId: id, sessionId: session, archived: true)
            for owned in state.sessions where owned.cwd == config.project && !owned.archived && owned.id != session {
                try await core.archiveSession(hostId: id, sessionId: owned.id, archived: true)
            }
            try await core.disconnect(); receiver.detach(); window?.close()
        } catch {
            try? await core.stopDesktop(hostId: try core.snapshot().selectedHost ?? ""); try? await core.disconnect(); receiver.detach(); window?.close()
            throw error
        }
    }
    @MainActor
    private func wait(_ core: KiteCore, seconds: Int = 30, until predicate: (CoreSnapshot) -> Bool) async throws -> CoreSnapshot {
        for _ in 0..<(seconds * 10) {
            let snapshot = try core.snapshot()
            if predicate(snapshot) { return snapshot }
            try await Task.sleep(for: .milliseconds(100))
        }
        let snapshot = try core.snapshot()
        throw failure("Timed out: connection=\(snapshot.connection), desktop=\(snapshot.desktop.status), details=\(snapshot.failure?.details ?? "")")
    }
    private func failure(_ message: String) -> NSError { NSError(domain: "KiteOwnedIntegration", code: 1, userInfo: [NSLocalizedDescriptionKey: message]) }
}
