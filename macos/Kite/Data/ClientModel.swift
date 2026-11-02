import AppKit
import Observation
import UniformTypeIdentifiers

private final class StateObserver: CoreObserver, @unchecked Sendable {
    var receive: (@Sendable (CoreSnapshot) -> Void)?
    func stateChanged(snapshot: CoreSnapshot) { receive?(snapshot) }
}

struct Destination: Equatable {
    let host: String?
    let agent: String?
    let conversation: String
}

@MainActor @Observable
final class ClientModel {
    private(set) var snapshot: CoreSnapshot?
    private(set) var messages: [MessageInfo] = []
    private(set) var messageRevision: UInt64 = 0
    private(set) var nativeError: String?
    private(set) var busy = false
    var draft = ""
    var showArchived = false
    var clipboardEnabled = UserDefaults.standard.bool(forKey: "clipboardEnabled") { didSet { UserDefaults.standard.set(clipboardEnabled, forKey: "clipboardEnabled") } }
    var muted = UserDefaults.standard.bool(forKey: "muted") { didSet { media.audio.muted = muted; UserDefaults.standard.set(muted, forKey: "muted") } }
    let media = MediaReceiver()
    @ObservationIgnored private var core: KiteCore?
    @ObservationIgnored private var initialization: Task<Void, Never>?
    @ObservationIgnored private var operation: Task<Void, Never>?
    @ObservationIgnored private var windowFocused = false
    @ObservationIgnored private var draftContext: Destination?
    @ObservationIgnored private var drafts: [DestinationKey: String] = [:]
    private struct DestinationKey: Hashable { let host: String?; let context: String }

    var destination: Destination { Destination(host: snapshot?.selectedHost, agent: snapshot?.selectedAgent, conversation: snapshot?.conversationContext ?? "") }
    var connected: Bool { snapshot?.connection == .connected }
    var canSend: Bool {
        guard connected, let state = snapshot, let id = state.selectedAgent,
              let agent = state.agents.first(where: { $0.id == id }), !agent.launchPending else { return false }
        let managed = state.providers.first(where: { $0.id == agent.agent })?.managedChat == true
        return !managed || state.composer.ready && !state.composer.loading
    }
    var selectedHost: HostRecord? { snapshot?.hosts.first { $0.id == snapshot?.selectedHost } }
    var selectedSession: SessionInfo? { snapshot?.sessions.first { $0.id == snapshot?.selectedSession } }

    init() {
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil { return }
        let observer = StateObserver()
        observer.receive = { [weak self] next in Task { @MainActor in self?.accept(next) } }
        do { core = try KiteCore(secretStore: DeviceKeyStore(), observer: observer) }
        catch { nativeError = String(describing: error) }
        media.attach()
        media.audio.muted = muted
        media.leaveOperation = { [weak self] in self?.takeover(false) }
    }

    func initialize() async {
        if snapshot?.initialized == true { return }
        if let initialization { await initialization.value; return }
        guard let core else { return }
        let task = Task { @MainActor in
            do {
                let folder = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                    .appendingPathComponent("io.github.wenshiqi0.kite.mac/core", isDirectory: true)
                try await core.initialize(dataDirectory: folder.path, deviceName: "kite@macOS", legacy: LegacyData(source: "macos-v1", host: nil, trustedKeys: []))
                accept(try core.snapshot())
            } catch { nativeError = String(describing: error) }
        }
        initialization = task
        await task.value
        initialization = nil
    }

    private func accept(_ next: CoreSnapshot) {
        guard snapshot == nil || next.revision > snapshot!.revision else { return }
        let previous = destination
        let nextDestination = Destination(host: next.selectedHost, agent: next.selectedAgent, conversation: next.conversationContext)
        if previous != nextDestination {
            drafts[DestinationKey(host: previous.host, context: previous.conversation)] = draft
            draft = drafts[DestinationKey(host: nextDestination.host, context: nextDestination.conversation)] ?? ""
            media.input.cancel()
        }
        let changed = next.conversationContext != snapshot?.conversationContext || next.conversationRevision != snapshot?.conversationRevision
        snapshot = next
        if changed { synchronizeConversation() }
        media.input.enabled = next.desktop.takeover && next.desktop.status == .streaming && NSApp.isActive && windowFocused
        if !media.input.enabled { media.input.cancel() }
    }

    private func synchronizeConversation() {
        guard let core, let state = snapshot else { return }
        let reset = draftContext?.conversation != state.conversationContext || draftContext?.host != state.selectedHost
        let delta = core.conversationSince(context: state.conversationContext, revision: reset ? 0 : messageRevision)
        guard delta.context == state.conversationContext else { return }
        if reset || delta.reset { messages.removeAll(keepingCapacity: true) }
        for edit in delta.edits {
            guard edit.at <= UInt64(messages.count), edit.remove <= UInt64(messages.count) - edit.at else { return }
            let at = Int(edit.at)
            messages.replaceSubrange(at..<(at + Int(edit.remove)), with: edit.items)
        }
        messageRevision = delta.revision
        draftContext = destination
    }

    /// Capture before the asynchronous hop; stale UI actions never reach a new target.
    func command(scoped: Bool = true, _ action: @escaping @MainActor (KiteCore) async throws -> Void) {
        guard let core else { return }
        let target = destination
        Task { @MainActor in
            guard !scoped || target == destination else { return }
            do { try await action(core) }
            catch { nativeError = String(describing: error) }
        }
    }

    func connect(_ id: String) {
        guard !busy, let core else { return }
        busy = true
        media.input.cancel()
        operation = Task { @MainActor in
            defer { busy = false }
            do { try await core.connect(hostId: id) }
            catch { nativeError = String(describing: error) }
        }
    }
    func save(_ host: HostRecord) async -> Bool {
        guard let core else { return false }
        do { _ = try await core.saveHost(host: host); return true }
        catch { nativeError = String(describing: error); return false }
    }
    func disconnect() {
        media.input.cancel()
        command { try await $0.disconnect() }
    }
    func open(_ session: SessionInfo) {
        guard !busy else { return }
        command { try await $0.openSession(sessionId: session.id) }
    }
    func send() {
        guard canSend, let id = snapshot?.selectedAgent, !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let text = draft, target = destination
        command { [weak self] core in
            try await core.promptAgent(agentId: id, text: text)
            guard let self else { return }
            if self.destination == target && self.draft == text { self.draft = "" }
            else { self.drafts[DestinationKey(host: target.host, context: target.conversation)] = "" }
        }
    }
    func desktop() {
        guard let state = snapshot else { return }
        if state.desktop.status == .streaming || state.desktop.status == .starting || state.desktop.status == .pairing {
            media.input.cancel()
            command { try await $0.stopDesktop() }
        } else { let handle = media.handle; command { try await $0.startDesktop(mediaTarget: handle) } }
    }
    func takeover(_ value: Bool) {
        if !value { media.input.enabled = false; media.input.cancel() }
        command { try await $0.setTakeover(enabled: value) }
    }
    func lostFocus() {
        windowFocused = false
        media.input.enabled = false
        media.input.cancel()
        if snapshot?.desktop.takeover == true { takeover(false) }
    }
    func gainedFocus() { windowFocused = true }
    func closeWorkspace() {
        lostFocus()
        command { try await $0.disconnect() }
    }
    func shutdown() async {
        lostFocus()
        try? await core?.disconnect()
        media.detach()
    }
    func clearError() {
        nativeError = nil
        command { try await $0.clearFailure() }
    }
    func upload(_ urls: [URL]) {
        guard let id = snapshot?.selectedAgent else { return }
        for url in urls {
            let mime = (try? url.resourceValues(forKeys: [.contentTypeKey]))?.contentType?.preferredMIMEType ?? "application/octet-stream"
            command { core in
                try await core.uploadAttachment(agentId: id, localPath: url.path, name: url.lastPathComponent, mime: mime, retryId: nil)
            }
        }
    }
    func synchronizeClipboard() async {
        guard clipboardEnabled, NSApp.isActive, let core, let host = snapshot?.selectedHost,
              snapshot?.desktop.takeover == true else { return }
        let target = destination, board = NSPasteboard.general, change = board.changeCount
        let text = board.string(forType: .string)
        do {
            let remote = try await core.synchronizeClipboard(hostId: host, local: text)
            guard clipboardEnabled, NSApp.isActive, destination == target, snapshot?.desktop.takeover == true,
                  board.changeCount == change, let remote, remote != text else { return }
            board.clearContents(); board.setString(remote, forType: .string)
        } catch { /* Transient clipboard failure must not interrupt Chat. */ }
    }
    func runClipboard() async {
        while !Task.isCancelled {
            await synchronizeClipboard()
            do { try await Task.sleep(for: .milliseconds(750)) } catch { return }
        }
    }
}
