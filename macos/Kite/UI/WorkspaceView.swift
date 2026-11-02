import SwiftUI

struct WorkspaceView: View {
    @Bindable var model: ClientModel
    @State private var editor: HostDraft?
    @State private var creatingSession = false
    @State private var desktopOnly = false
    @State private var visibility: NavigationSplitViewVisibility = .all
    @Environment(\.scenePhase) private var phase
    var body: some View {
        NavigationSplitView(columnVisibility: $visibility) {
            sidebar.navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 330)
        } detail: {
            VStack(spacing: 0) {
                if let error = model.nativeError ?? model.snapshot?.failure?.details {
                    HStack(alignment: .top) {
                        Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(tr("action_failed")).fontWeight(.medium)
                            DisclosureGroup(tr("details")) { Text(error).font(.caption.monospaced()).textSelection(.enabled) }
                        }
                        Spacer()
                        Button { model.clearError() } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
                    }.padding(12).background(.orange.opacity(0.08))
                    Divider()
                }
                if model.connected {
                    if desktopOnly { DesktopPane(model: model, expanded: $desktopOnly) }
                    else {
                        HSplitView {
                            ChatPane(model: model).frame(minWidth: 340, idealWidth: 520)
                            DesktopPane(model: model, expanded: $desktopOnly).frame(minWidth: 340, idealWidth: 640)
                        }
                    }
                } else { disconnected }
            }
            .navigationTitle(model.selectedSession?.title.isEmpty == false ? model.selectedSession!.title : "Kite")
            .navigationSubtitle(model.selectedHost.map { "\($0.username)@\($0.host)" } ?? tr("native_workspace"))
            .toolbar {
                ToolbarItemGroup {
                    if model.connected {
                        Button { creatingSession = true } label: { Label(tr("new_session"), systemImage: "square.and.pencil") }
                        Button { model.disconnect() } label: { Label(tr("disconnect"), systemImage: "bolt.slash") }
                    }
                    SettingsLink { Image(systemName: "gearshape") }
                }
            }
        }
        .background(WindowLifecycle(model: model).frame(width: 0, height: 0))
        .tint(Color(red: 0.25, green: 0.78, blue: 0.77))
        .sheet(item: $editor) { draft in HostEditor(model: model, draft: draft) }
        .sheet(isPresented: $creatingSession) { SessionEditor(model: model) }
        .sheet(isPresented: Binding(get: { model.snapshot?.pendingHostKey != nil }, set: { _ in })) {
            if let pending = model.snapshot?.pendingHostKey { TrustSheet(model: model, pending: pending) }
        }
        .onChange(of: phase) { _, next in if next != .active { model.lostFocus() } }
        .onChange(of: desktopOnly) { _, value in visibility = value ? .detailOnly : .all }
    }
    private var sidebar: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "paperplane.fill").font(.title2).foregroundStyle(.tint)
                VStack(alignment: .leading, spacing: 1) {
                    Text("Kite").font(.title3.weight(.semibold))
                    Text(tr("remote_workspace")).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button { editor = HostDraft() } label: { Image(systemName: "plus") }.help(tr("add_host"))
            }.padding(18)
            List {
                Section(tr("machines")) {
                    ForEach(model.snapshot?.hosts ?? [], id: \.id) { host in
                        HStack(alignment: .top, spacing: 10) {
                            Image(systemName: "desktopcomputer").foregroundStyle(host.id == model.snapshot?.selectedHost && model.connected ? .cyan : .secondary).padding(.top, 3)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(host.host).fontWeight(.medium).lineLimit(1)
                                Text(host.id == model.snapshot?.selectedHost ? connectionLabel : tr("disconnected"))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 2)
                            Button { editor = HostDraft(host) } label: { Image(systemName: "pencil") }
                                .buttonStyle(.borderless).help(tr("edit_host"))
                            Button { model.connect(host.id) } label: { Image(systemName: host.id == model.snapshot?.selectedHost && model.connected ? "checkmark.circle.fill" : "arrow.right.circle") }
                                .buttonStyle(.borderless).disabled(model.busy || model.snapshot?.connection == .connecting)
                                .help(tr("connect"))
                        }.padding(.vertical, 7)
                            .contextMenu { Button(tr("edit_host")) { editor = HostDraft(host) } }
                    }
                    if model.snapshot?.hosts.isEmpty != false { Button(tr("add_host")) { editor = HostDraft() } }
                }
                if model.connected {
                    Section {
                        ForEach((model.snapshot?.sessions ?? []).filter { $0.archived == model.showArchived }, id: \.id) { session in
                            Button { model.open(session) } label: {
                                HStack(alignment: .top, spacing: 8) {
                                    Image(systemName: session.id == model.snapshot?.selectedSession ? "bubble.left.fill" : "bubble.left")
                                        .foregroundStyle(session.id == model.snapshot?.selectedSession ? .cyan : .secondary)
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(session.title.isEmpty ? tr("untitled_session") : session.title).font(.callout).lineLimit(1)
                                        Text(session.preview.isEmpty ? session.provider : session.preview).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                    }
                                }.padding(.vertical, 5).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                            }.buttonStyle(.plain)
                            .contextMenu {
                                Button(tr(session.archived ? "restore_session" : "archive_session")) {
                                    model.command { try await $0.archiveSession(sessionId: session.id, archived: !session.archived) }
                                }
                            }
                        }
                    } header: {
                        HStack {
                            Text(tr("sessions")); Spacer()
                            Button { creatingSession = true } label: { Image(systemName: "plus") }.buttonStyle(.plain)
                        }
                    }
                }
            }.listStyle(.sidebar)
            Spacer(minLength: 0)
            if model.connected {
                Toggle(tr("archived_sessions"), isOn: $model.showArchived).toggleStyle(.checkbox).font(.caption).padding(14)
            }
            Divider()
            HStack {
                Circle().fill(model.connected ? .green : .secondary).frame(width: 6, height: 6)
                Text(connectionLabel).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Text("macOS").font(.caption2).foregroundStyle(.tertiary)
            }.padding(14)
        }
    }
    private var connectionLabel: String {
        switch model.snapshot?.connection {
        case .connected: tr("connected")
        case .connecting: tr("connecting")
        case .awaitingTrust: tr("awaiting_trust")
        case .failed: tr("connection_failed")
        default: tr("disconnected")
        }
    }
    private var disconnected: some View {
        VStack(spacing: 20) {
            Image(systemName: "laptopcomputer.and.arrow.down").font(.system(size: 56, weight: .ultraLight)).foregroundStyle(.tint)
            Text(tr("welcome_title")).font(.largeTitle.weight(.semibold))
            Text(tr("welcome_description")).foregroundStyle(.secondary).multilineTextAlignment(.center).frame(maxWidth: 440)
            if model.snapshot?.initialized != true {
                if model.nativeError != nil {
                    Button(tr("retry")) { Task { await model.initialize() } }.buttonStyle(.borderedProminent)
                } else { ProgressView(tr("initializing_identity")) }
            } else if model.snapshot?.connection == .connecting { ProgressView(tr("connecting")) }
            else if let host = model.selectedHost {
                Button(tr("connect")) { model.connect(host.id) }.buttonStyle(.borderedProminent).controlSize(.large)
            } else { Button(tr("add_host")) { editor = HostDraft() }.buttonStyle(.borderedProminent).controlSize(.large) }
        }.frame(maxWidth: .infinity, maxHeight: .infinity).padding(36)
    }
}

struct DesktopPane: View {
    @Bindable var model: ClientModel
    @Binding var expanded: Bool
    @State private var zoom: CGFloat = 1
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Label(tr("remote_desktop"), systemImage: "display").font(.callout.weight(.medium))
                Spacer()
                if model.snapshot?.desktop.status == .streaming {
                    Text("\(model.snapshot?.desktop.width ?? 0) × \(model.snapshot?.desktop.height ?? 0)")
                        .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                Button { expanded.toggle() } label: { Image(systemName: expanded ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right") }.buttonStyle(.plain).help(tr("expand_desktop"))
            }.padding(14)
            Divider()
            ZStack {
                Color.black.opacity(0.7)
                if let desktop = model.snapshot?.desktop, desktop.status == .streaming {
                    DesktopSurface(receiver: model.media, source: CGSize(width: Int(desktop.width), height: Int(desktop.height)), zoom: zoom, takeover: desktop.takeover)
                } else {
                    VStack(spacing: 16) {
                        Image(systemName: "display").font(.system(size: 46, weight: .ultraLight)).foregroundStyle(.secondary)
                        if model.snapshot?.desktop.status == .pairing {
                            Text(tr("pairing_instruction")).multilineTextAlignment(.center)
                            Text(model.snapshot?.desktop.pin ?? "").font(.system(size: 36, weight: .medium, design: .monospaced)).textSelection(.enabled)
                            Button(tr("approve_pairing")) {
                                if let pin = model.snapshot?.desktop.pin { model.command { try await $0.approvePairing(pin: pin) } }
                            }
                        } else if model.snapshot?.desktop.status == .starting || model.snapshot?.desktop.status == .stopping {
                            ProgressView(tr("desktop_connecting"))
                            Button(tr("cancel")) { model.desktop() }
                        } else {
                            Text(tr(model.snapshot?.desktop.status == .failed ? "desktop_failed" : "desktop_description")).foregroundStyle(.secondary).multilineTextAlignment(.center)
                            Button(tr("open_desktop")) { model.desktop() }.buttonStyle(.borderedProminent)
                        }
                    }.padding(28)
                }
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider()
            HStack {
                Picker(tr("input_mode"), selection: Binding(get: { model.snapshot?.desktop.takeover == true }, set: { model.takeover($0) })) {
                    Text(tr("watch")).tag(false); Text(tr("operate")).tag(true)
                }.pickerStyle(.segmented).frame(width: 150).disabled(model.snapshot?.desktop.status != .streaming)
                Spacer()
                Button { zoom = max(1, zoom - 0.25) } label: { Image(systemName: "minus.magnifyingglass") }.buttonStyle(.plain)
                Button("\(Int(zoom * 100))%") { zoom = 1 }.buttonStyle(.plain).font(.caption.monospacedDigit())
                Button { zoom = min(3, zoom + 0.25) } label: { Image(systemName: "plus.magnifyingglass") }.buttonStyle(.plain)
                Button { model.muted.toggle() } label: { Image(systemName: model.muted ? "speaker.slash" : "speaker.wave.2") }.buttonStyle(.plain).help(tr("mute"))
                if model.snapshot?.desktop.status == .streaming { Button { model.desktop() } label: { Image(systemName: "stop.circle") }.buttonStyle(.plain).help(tr("stop_desktop")) }
            }.padding(14)
        }
        .background(Color(nsColor: .underPageBackgroundColor))
    }
}
