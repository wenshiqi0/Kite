import SwiftUI

struct HostDraft: Identifiable {
    let id = UUID()
    var hostID = ""
    var address = ""
    var port = "22"
    var username = NSUserName()
    var herdr = ""
    var streamHost = ""
    var sunshinePort = "47989"
    init() {}
    init(_ host: HostRecord) {
        hostID = host.id; address = host.host; port = String(host.port); username = host.username
        herdr = host.herdrPath ?? ""; streamHost = host.streamHost ?? ""; sunshinePort = String(host.sunshinePort)
    }
}

struct HostEditor: View {
    let model: ClientModel
    @State var draft: HostDraft
    @State private var validation: String?
    @State private var saving = false
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(tr(draft.hostID.isEmpty ? "add_host" : "edit_host")).font(.title2.weight(.semibold))
            Form {
                TextField(tr("host_address"), text: $draft.address)
                TextField(tr("ssh_port"), text: $draft.port)
                TextField(tr("ssh_username"), text: $draft.username)
                DisclosureGroup(tr("advanced")) {
                    TextField(tr("herdr_path"), text: $draft.herdr)
                    TextField(tr("stream_address"), text: $draft.streamHost)
                    TextField(tr("sunshine_port"), text: $draft.sunshinePort)
                }
            }.textFieldStyle(.roundedBorder)
            Text(tr("host_setup_hint")).font(.callout).foregroundStyle(.secondary)
            if let key = model.snapshot?.deviceKey {
                Text(key.publicKeyOpenssh).font(.caption.monospaced()).textSelection(.enabled).lineLimit(3)
                Button(tr("copy_public_key")) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(key.publicKeyOpenssh, forType: .string) }
            }
            if let validation { Text(validation).foregroundStyle(.orange).font(.callout) }
            HStack {
                Spacer()
                Button(tr("cancel")) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(tr("save")) { save() }.buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction).disabled(saving || model.snapshot?.initialized != true)
            }
        }.padding(28).frame(width: 510)
    }
    private func save() {
        let address = draft.address.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = draft.username.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !address.isEmpty, !username.isEmpty, let port = UInt16(draft.port), port > 0,
              let sunshine = UInt16(draft.sunshinePort), sunshine > 0 else { validation = tr("invalid_host"); return }
        let record = HostRecord(id: draft.hostID, host: address, port: port, username: username,
            herdrPath: draft.herdr.isEmpty ? nil : draft.herdr, streamHost: draft.streamHost.isEmpty ? nil : draft.streamHost, sunshinePort: sunshine)
        saving = true
        Task { if await model.save(record) { dismiss() }; saving = false }
    }
}

struct TrustSheet: View {
    let model: ClientModel
    let pending: HostKeyConfirmation
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Label(tr("verify_host"), systemImage: "lock.shield").font(.title2.weight(.semibold))
            Text(pending.address).font(.headline)
            Text(tr("trust_instruction")).foregroundStyle(.secondary)
            Text(pending.fingerprint).font(.body.monospaced()).textSelection(.enabled)
            HStack {
                Spacer()
                Button(tr("cancel")) { model.disconnect() }.keyboardShortcut(.cancelAction)
                Button(tr("trust_connect")) {
                    model.command { try await $0.trustHost(hostId: pending.hostId, fingerprint: pending.fingerprint) }
                }.buttonStyle(.borderedProminent)
            }
        }.padding(28).frame(width: 540).interactiveDismissDisabled()
    }
}

struct SessionEditor: View {
    let model: ClientModel
    @State private var provider = ""
    @State private var title = ""
    @State private var cwd = ""
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(tr("new_session")).font(.title2.weight(.semibold))
            Form {
                Picker(tr("harness"), selection: $provider) {
                    ForEach(model.snapshot?.providers ?? [], id: \.id) { Text($0.id).tag($0.id) }
                }
                TextField(tr("session_title"), text: $title)
                TextField(tr("working_directory"), text: $cwd)
            }.textFieldStyle(.roundedBorder)
            HStack {
                Spacer(); Button(tr("cancel")) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(tr("create")) {
                    let provider = provider, title = title, cwd = cwd
                    model.command { try await $0.createSession(provider: provider, cwd: cwd, title: title) }
                    dismiss()
                }.buttonStyle(.borderedProminent).disabled(provider.isEmpty)
            }
        }.padding(28).frame(width: 460)
            .onAppear { provider = model.snapshot?.providers.first(where: { $0.id == "codex" })?.id ?? model.snapshot?.providers.first?.id ?? "" }
    }
}

struct SettingsView: View {
    @Bindable var model: ClientModel
    var body: some View {
        Form {
            Section(tr("desktop_settings")) {
                Toggle(tr("clipboard_sync"), isOn: $model.clipboardEnabled)
                Text(tr("clipboard_hint")).font(.caption).foregroundStyle(.secondary)
                Toggle(tr("mute"), isOn: $model.muted)
            }
            Section(tr("device_identity")) {
                Text(tr("identity_hint")).foregroundStyle(.secondary)
                if let key = model.snapshot?.deviceKey {
                    Text(key.fingerprint).font(.caption.monospaced()).textSelection(.enabled)
                    Button(tr("copy_public_key")) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(key.publicKeyOpenssh, forType: .string) }
                }
            }
        }.formStyle(.grouped).padding(12).frame(width: 520, height: 360)
    }
}
