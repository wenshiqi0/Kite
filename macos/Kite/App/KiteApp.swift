import SwiftUI

@main
struct KiteApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var model = ClientModel()
    var body: some Scene {
        Window("Kite", id: "workspace") {
            WorkspaceView(model: model)
                .preferredColorScheme(.dark)
                .frame(minWidth: 920, minHeight: 640)
                .task { delegate.model = model; await model.initialize() }
                .task { await model.runClipboard() }
        }
        .defaultSize(width: 1400, height: 880)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandMenu(tr("connection")) {
                Button(tr("disconnect")) { model.disconnect() }.disabled(!model.connected)
                Button(tr("watch")) { model.takeover(false) }.keyboardShortcut("w", modifiers: [.command, .option])
            }
        }
        Settings { SettingsView(model: model).preferredColorScheme(.dark) }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    var model: ClientModel?
    private var terminating = false
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard !terminating, let model else { return .terminateNow }
        terminating = true
        Task {
            await model.shutdown()
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}

struct WindowLifecycle: NSViewRepresentable {
    let model: ClientModel
    func makeNSView(context: Context) -> LifecycleView { LifecycleView(model: model) }
    func updateNSView(_ view: LifecycleView, context: Context) {}
}
@MainActor
final class LifecycleView: NSView {
    let model: ClientModel
    private var observers: [NSObjectProtocol] = []
    init(model: ClientModel) { self.model = model; super.init(frame: .zero) }
    required init?(coder: NSCoder) { nil }
    deinit { observers.forEach(NotificationCenter.default.removeObserver) }
    override func viewDidMoveToWindow() {
        observers.forEach(NotificationCenter.default.removeObserver); observers.removeAll()
        guard let window else { return }
        if window.isKeyWindow { model.gainedFocus() }
        observers.append(NotificationCenter.default.addObserver(forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.model.gainedFocus() }
        })
        observers.append(NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.model.lostFocus() }
        })
        observers.append(NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.model.closeWorkspace() }
        })
    }
}
