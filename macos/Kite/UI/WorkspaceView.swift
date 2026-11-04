import SwiftUI

struct WorkspaceView: View {
    @Bindable var model: ClientModel
    @State private var editor: HostDraft?
    @State private var creatingSession = false
    @State private var sessionSearch = ""
    @State private var collapsedMachines: Set<String> = []
    @State private var desktopOnly = false
    @State private var visibility: NavigationSplitViewVisibility = .all
    @Environment(\.scenePhase) private var phase
    @Environment(\.openWindow) private var openWindow
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
                    if model.recovering {
                        Button(tr("cancel_reconnect")) { model.disconnect() }
                    }
                    if model.connected {
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
            HStack {
                Text(tr("sessions")).font(.title3.weight(.semibold))
                Spacer()
            }.padding(.horizontal, 14).padding(.top, 12).padding(.bottom, 8)
            HStack(spacing: 8) {
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField(tr("search_sessions"), text: $sessionSearch).textFieldStyle(.plain)
                }.font(.callout).padding(.horizontal, 9).frame(height: 32)
                    .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 7))
                Button { creatingSession = true } label: { Image(systemName: "plus").font(.title3) }
                    .buttonStyle(.plain).frame(width: 30, height: 32).help(tr("new_session"))
                    .disabled(model.busy || model.snapshot?.hostConnections.contains { $0.connection == .connected } != true)
            }.padding(.horizontal, 12)
            HStack {
                Picker(tr("sessions"), selection: $model.showArchived) {
                    Text(tr("active_sessions")).tag(false)
                    Text(tr("archived_sessions")).tag(true)
                }.pickerStyle(.segmented).labelsHidden()
                Menu {
                    Button(tr("sort_newest")) { model.command(scoped: false) { try await $0.setWorkspaceSort(order: .newest) } }
                    Button(tr("sort_oldest")) { model.command(scoped: false) { try await $0.setWorkspaceSort(order: .oldest) } }
                    Button(tr("sort_activity")) { model.command(scoped: false) { try await $0.setWorkspaceSort(order: .recentActivity) } }
                    Button(tr("sort_title")) { model.command(scoped: false) { try await $0.setWorkspaceSort(order: .title) } }
                } label: { Image(systemName: "line.3.horizontal.decrease") }
                    .menuStyle(.borderlessButton).fixedSize().help(tr("session_sort"))
            }.padding(.horizontal, 12).padding(.vertical, 8)
            List {
                ForEach(model.snapshot?.hosts ?? [], id: \.id) { host in
                    let workspace = model.snapshot?.hostConnections.first { $0.hostId == host.id }
                    let connection = workspace?.connection ?? .disconnected
                    let sessions = (workspace?.sessions ?? []).filter {
                        $0.archived == model.showArchived && (sessionSearch.isEmpty || host.host.localizedCaseInsensitiveContains(sessionSearch) || $0.title.localizedCaseInsensitiveContains(sessionSearch) || $0.preview.localizedCaseInsensitiveContains(sessionSearch) || $0.provider.localizedCaseInsensitiveContains(sessionSearch) || $0.subagents.contains { $0.name.localizedCaseInsensitiveContains(sessionSearch) || $0.task.localizedCaseInsensitiveContains(sessionSearch) })
                    }
                    let expanded = !collapsedMachines.contains(host.id) || !sessionSearch.isEmpty
                    if sessionSearch.isEmpty || !sessions.isEmpty {
                        Section {
                            if expanded {
                                ForEach(sessions, id: \.id) { session in
                                    let selected = model.snapshot?.selectedHost == host.id && model.snapshot?.selectedSession == session.id
                                    Button { model.open(session, hostId: host.id) } label: {
                                        HStack(alignment: .top, spacing: 8) {
                                            Image(systemName: selected ? "bubble.left.fill" : "bubble.left").foregroundStyle(selected ? .cyan : .secondary)
                                            VStack(alignment: .leading, spacing: 3) {
                                                Text(session.title.isEmpty ? tr("untitled_session") : session.title).font(.callout).lineLimit(1)
                                                Text(session.preview.isEmpty ? session.provider : session.preview).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                            }
                                        }.padding(.vertical, 3).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                                    }.buttonStyle(.plain)
                                    .listRowBackground(selected ? Color.cyan.opacity(0.12) : Color.clear)
                                    .contextMenu {
                                        Button(tr(session.archived ? "restore_session" : "archive_session")) {
                                            model.command(scoped: false) { try await $0.archiveSession(hostId: host.id, sessionId: session.id, archived: !session.archived) }
                                        }.disabled(connection != .connected)
                                    }
                                    SubagentTree(agents: session.subagents, failed: session.subagentsError, connected: connection == .connected)
                                        .id("\(host.id)/\(session.id)").padding(.leading, 16)
                                }
                                if sessions.isEmpty { Text(tr("no_sessions")).font(.caption).foregroundStyle(.secondary) }
                            }
                        } header: {
                            HStack(spacing: 6) {
                                Button {
                                    if expanded { collapsedMachines.insert(host.id) } else { collapsedMachines.remove(host.id) }
                                } label: {
                                    HStack(spacing: 6) {
                                        Image(systemName: "chevron.right").font(.caption2).rotationEffect(.degrees(expanded ? 90 : 0))
                                        Circle().fill(connection == .connected ? Color.green : Color.secondary).frame(width: 5, height: 5)
                                        Text(host.host).lineLimit(1)
                                    }
                                }.buttonStyle(.plain)
                                Spacer(minLength: 0)
                                Text(String(sessions.count)).font(.caption2)
                                Menu {
                                    Button(tr("machine_settings")) { editor = HostDraft(host) }
                                    if connection == .connected {
                                        Button(tr("disconnect")) { model.command(scoped: false) { try await $0.disconnectHost(hostId: host.id) } }
                                    } else { Button(tr("connect")) { model.connect(host.id) } }
                                } label: { Image(systemName: "ellipsis") }
                                    .menuStyle(.borderlessButton).fixedSize().help(tr("machine_settings"))
                            }.font(.caption)
                        }.id(host.id)
                    }
                }
            }.listStyle(.sidebar)
            Divider()
            HStack {
                Button { editor = HostDraft() } label: { Label(tr("add_host"), systemImage: "plus") }.buttonStyle(.plain)
                Spacer()
                Button { openWindow(id: "local-host") } label: { Image(systemName: "server.rack") }.buttonStyle(.plain).help(tr("local_host"))
            }.font(.caption).padding(14)
        }
    }
    private var connectionLabel: String {
        connectionLabel(for: model.snapshot?.connection ?? .disconnected)
    }
    private func connectionLabel(for connection: ConnectionStatus) -> String {
        switch connection {
        case .connected: tr("connected")
        case .connecting: tr("connecting")
        case .reconnecting: tr("reconnecting")
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
                // The first presented frame advances Core from Starting to
                // Streaming, so the native surface must exist during setup.
                if let desktop = model.snapshot?.desktop,
                   [.starting, .streaming, .stopping].contains(desktop.status) {
                    DesktopSurface(receiver: model.media, source: CGSize(width: Int(desktop.width), height: Int(desktop.height)), zoom: zoom, takeover: desktop.takeover)
                }
                if model.snapshot?.desktop.status != .streaming {
                    VStack(spacing: 16) {
                        Image(systemName: "display").font(.system(size: 46, weight: .ultraLight)).foregroundStyle(.secondary)
                        if model.snapshot?.desktop.status == .pairing {
                            Text(tr("pairing_instruction")).multilineTextAlignment(.center)
                            Text(model.snapshot?.desktop.pin ?? "").font(.system(size: 36, weight: .medium, design: .monospaced)).textSelection(.enabled)
                            Button(tr("approve_pairing")) {
                                if let pin = model.snapshot?.desktop.pin, let host = model.snapshot?.selectedHost { model.command { try await $0.approvePairing(hostId: host, pin: pin) } }
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
