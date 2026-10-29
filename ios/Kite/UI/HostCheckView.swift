import SwiftUI
import UIKit

/// M1 screen. Layout follows the width size class, not the device: an iPad in
/// Split View or Slide Over is compact and gets the iPhone layout.
struct HostCheckView: View {
    @State private var model = HostCheckModel()
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass

    var body: some View {
        NavigationStack {
            Group {
                if horizontalSizeClass == .regular {
                    regularLayout
                } else {
                    compactLayout
                }
            }
            .navigationTitle("app_name")
        }
        .onAppear(perform: model.loadDeviceKey)
        .alert("host_key_title", isPresented: hostKeyPrompt, presenting: model.pendingHostKey) { confirmation in
            Button("host_key_trust") { model.trustHostKey(confirmation) }
            Button("cancel", role: .cancel, action: model.dismissHostKey)
        } message: { confirmation in
            Text(String(
                format: NSLocalizedString("host_key_body", comment: ""),
                confirmation.address,
                confirmation.fingerprint
            ))
        }
    }

    /// iPhone, and iPad in a narrow window: one column.
    private var compactLayout: some View {
        Form {
            DeviceKeySection(model: model)
            HostFormSection(model: model)
            ResultSection(model: model)
        }
    }

    /// iPad with room to spare: setup on the left, results on the right.
    private var regularLayout: some View {
        HStack(spacing: 0) {
            Form {
                DeviceKeySection(model: model)
                HostFormSection(model: model)
            }
            .frame(maxWidth: 480)
            Divider()
            Group {
                if model.state == .idle || model.pendingHostKey != nil {
                    ContentUnavailableView(
                        "result_placeholder_title",
                        systemImage: "network",
                        description: Text("result_placeholder_body")
                    )
                } else {
                    Form { ResultSection(model: model) }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var hostKeyPrompt: Binding<Bool> {
        Binding(
            get: { model.pendingHostKey != nil },
            set: { _ in }
        )
    }
}

private struct DeviceKeySection: View {
    let model: HostCheckModel
    @State private var copied = false

    var body: some View {
        Section {
            if let error = model.deviceKeyError {
                Text(String(format: NSLocalizedString("device_key_error", comment: ""), error))
                    .foregroundStyle(.red)
            } else if let key = model.deviceKey {
                Text(key.fingerprint)
                    .font(.caption.monospaced())
                Text(key.publicKeyOpenssh)
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
                Button {
                    UIPasteboard.general.string = key.publicKeyOpenssh
                    copied = true
                } label: {
                    Label(copied ? "device_key_copied" : "device_key_copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                }
            } else {
                Text("device_key_generating")
            }
        } header: {
            Text("device_key_title")
        } footer: {
            Text("device_key_hint")
        }
    }
}

private struct HostFormSection: View {
    @Bindable var model: HostCheckModel

    var body: some View {
        Section("host_title") {
            TextField("host_address", text: $model.form.host)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            TextField("host_port", text: $model.form.port)
                .keyboardType(.numberPad)
            TextField("host_username", text: $model.form.username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            TextField("host_herdr_path", text: $model.form.herdrPath)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button(action: model.check) {
                Text(model.isConnected ? "disconnect" : (model.isRunning ? "check_running" : "check_herdr"))
            }
            .disabled(model.deviceKey == nil || model.isRunning)
            // ⌘↩ with a hardware keyboard, on iPad or iPhone.
            .keyboardShortcut(.return, modifiers: .command)
        }
    }
}

private struct ResultSection: View {
    let model: HostCheckModel

    var body: some View {
        switch model.state {
        case .idle, .confirmHostKey:
            EmptyView()
        case .running:
            Section {
                HStack(spacing: 12) {
                    ProgressView()
                    Text("check_running")
                }
            }
        case let .passed(check):
            Section("check_passed") {
                DetailRow(label: "check_capability", value: check.capability)
                DetailRow(label: "check_path", value: check.herdrPath)
                if let version = check.version {
                    DetailRow(label: "check_version", value: version)
                }
                DetailRow(label: "check_host_key", value: check.hostKeyFingerprint)
            }
        case let .failed(message, hostKeyChanged):
            Section {
                Text(message)
                    .foregroundStyle(.red)
                if let details = model.failureDetails {
                    DisclosureGroup("details") { Text(details).font(.caption.monospaced()).textSelection(.enabled) }
                }
                if hostKeyChanged {
                    Button("forget_host_key", role: .destructive, action: model.forgetHostKey)
                }
            }
        }
    }
}

private struct DetailRow: View {
    let label: LocalizedStringKey
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.monospaced())
                .textSelection(.enabled)
        }
    }
}
