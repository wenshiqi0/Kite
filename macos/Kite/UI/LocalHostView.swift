import SwiftUI

struct LocalHostView: View {
    @Bindable var host: LocalHostModel
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .top, spacing: 14) {
                    Image(systemName: "desktopcomputer.and.arrow.down").font(.system(size: 32)).foregroundStyle(.orange)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(tr("local_host")).font(.title2.bold())
                        Text(tr("host_introduction")).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button { Task { await host.refresh() } } label: { Image(systemName: "arrow.clockwise") }.help(tr("refresh"))
                        .disabled(host.busy || host.refreshing)
                }
                if let error = host.error {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack { Text(tr("action_failed")).fontWeight(.semibold); Spacer(); Button { host.clearError() } label: { Image(systemName: "xmark") }.buttonStyle(.plain) }
                        Text(error).font(.caption.monospaced()).textSelection(.enabled)
                    }.padding(12).background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 10))
                }
                if let report = host.report {
                    GroupBox {
                        VStack(alignment: .leading, spacing: 12) {
                            state(tr("host_chat_service"), online: report.chatAvailable, detail: report.node.map { "PID \($0.pid) · \($0.version)" } ?? tr("host_not_running"))
                            ForEach(report.node?.services ?? []) { service in
                                state(service.name, online: service.active, detail: service.error ?? (service.ownership == "external" ? tr("host_external_service") : tr("host_background_service")))
                            }
                            Divider()
                            HStack {
                                if report.node != nil {
                                    Button(tr("host_disable")) { host.run("disable") }
                                    Button(tr("host_update")) { host.run("update") }
                                } else if report.configured {
                                    Button(tr("host_enable")) { host.run("enable") }.buttonStyle(.borderedProminent)
                                    Button(tr("host_reinstall")) { host.run("update") }
                                } else {
                                    Button(tr("host_install")) { host.run("install") }.buttonStyle(.borderedProminent)
                                        .disabled(report.dependencies.herdr == nil || report.dependencies.bun == nil)
                                }
                                if host.busy { ProgressView().controlSize(.small) }
                            }.disabled(host.busy || host.refreshing)
                            Text(tr("host_lifetime")).font(.caption).foregroundStyle(.secondary)
                        }.padding(8)
                    } label: { Label(tr("host_chat_service"), systemImage: "bubble.left.and.bubble.right") }
                    GroupBox {
                        VStack(alignment: .leading, spacing: 12) {
                            state(tr("host_ssh"), online: report.sshAvailable, detail: report.sshAvailable ? tr("host_ssh_online") : tr("host_ssh_setup"))
                            LabeledContent(tr("host_address")) { Text(report.hostname).textSelection(.enabled).font(.body.monospaced()) }
                            LabeledContent(tr("username")) { Text(report.username).textSelection(.enabled) }
                            LabeledContent(tr("ssh_port")) { Text("\(report.sshPort)") }
                            Text(tr("host_device_auth")).font(.caption).foregroundStyle(.secondary)
                            Button(tr("host_open_sharing")) { open("x-apple.systempreferences:com.apple.Sharing-Settings.extension") }
                        }.padding(8)
                    } label: { Label(tr("host_remote_access"), systemImage: "network") }
                    GroupBox {
                        VStack(alignment: .leading, spacing: 12) {
                            state("Sunshine", online: report.desktopAvailable, detail: report.desktopAvailable ? tr("host_stream_online") : (report.dependencies.sunshine == nil ? tr("host_sunshine_missing") : tr("host_stream_offline")))
                            HStack {
                                if report.dependencies.sunshine != nil {
                                    Button(tr("host_start_sunshine")) { host.run("sunshine") }.disabled(host.busy)
                                    Button(tr("host_sunshine_settings")) { open("https://localhost:47990") }
                                } else { Link(tr("host_install_sunshine"), destination: URL(string: "https://docs.lizardbyte.dev/projects/sunshine/latest/md_docs_2getting__started.html#macos")!) }
                                Button(tr("host_privacy_settings")) { open("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension") }
                            }
                            Text(tr("host_sunshine_permissions")).font(.caption).foregroundStyle(.secondary)
                        }.padding(8)
                    } label: { Label(tr("remote_desktop"), systemImage: "display") }
                    DisclosureGroup(tr("host_components")) {
                        VStack(alignment: .leading, spacing: 8) {
                            component("Herdr", report.dependencies.herdr)
                            component("Bun", report.dependencies.bun)
                            if let path = report.hostApplication {
                                Text(tr("host_file_access")).font(.caption).foregroundStyle(.secondary)
                                Button(tr("host_show_component")) { NSWorkspace.shared.selectFile(path, inFileViewerRootedAtPath: "") }
                            }
                            if report.dependencies.herdr == nil { Link(tr("host_install_herdr"), destination: URL(string: "https://herdr.dev/tui/docs/install/")!) }
                            Text(tr("host_platform_limits")).font(.caption).foregroundStyle(.secondary)
                        }.padding(.top, 10)
                    }
                } else if host.refreshing { ProgressView(tr("loading")) }
                if let date = host.updatedAt { Text("\(tr("host_updated")) \(date.formatted(date: .omitted, time: .standard))").font(.caption2).foregroundStyle(.tertiary) }
            }.padding(24)
        }.frame(minWidth: 620, minHeight: 600).task { await host.poll() }
    }
    private func state(_ name: String, online: Bool, detail: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: online ? "checkmark.circle.fill" : "circle.dashed").foregroundStyle(online ? .green : .secondary)
            VStack(alignment: .leading, spacing: 4) { Text(name).fontWeight(.medium); Text(detail).font(.caption).foregroundStyle(.secondary) }
            Spacer()
        }
    }
    private func component(_ name: String, _ path: String?) -> some View {
        LabeledContent(name) { Text(path ?? tr("host_missing")).font(.caption.monospaced()).textSelection(.enabled) }
    }
    private func open(_ value: String) { if let url = URL(string: value) { NSWorkspace.shared.open(url) } }
}
