import SwiftUI

struct SubagentTree: View {
    let agents: [SubagentInfo]
    let failed: Bool
    let connected: Bool
    var inspect: ((SubagentInfo) -> Void)? = nil
    @State private var expanded = false

    var body: some View {
        if !agents.isEmpty || failed {
            DisclosureGroup(isExpanded: $expanded) {
                if failed { Text(tr("subagents_unavailable")).foregroundStyle(.orange).font(.caption) }
                ForEach(agents, id: \.id) { task in
                    if let inspect, task.sessionId != nil {
                        Button { inspect(task) } label: { row(task) }.buttonStyle(.plain)
                    } else {
                        row(task)
                    }
                }
            } label: {
                Text(String(format: tr("subagents_count"), agents.count)).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private func row(_ task: SubagentInfo) -> some View {
        let available = task.connected ?? connected
        return HStack(spacing: 6) {
            VStack(alignment: .leading, spacing: 2) {
                Text(task.name).font(.caption).lineLimit(1)
                let detail = [task.machine, task.provider.map { providerLabel($0) }, task.task.isEmpty ? nil : task.task]
                    .compactMap { $0 }.joined(separator: " · ")
                if !detail.isEmpty { Text(detail).font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
            }
            Spacer(minLength: 4)
            Text(tr(statusKey(task.status, available: available, unreliable: failed && task.sessionId == nil))).font(.caption2)
                .foregroundStyle(statusColor(task.status, available: available, unreliable: failed && task.sessionId == nil))
        }.padding(.vertical, 3).contentShape(Rectangle())
    }

    private func statusKey(_ status: String, available: Bool, unreliable: Bool) -> String {
        if !available { return "subagent_disconnected" }
        if unreliable && ["working", "starting", "blocked", "question"].contains(status) { return "subagent_unknown" }
        switch status {
        case "working": return "subagent_running"
        case "starting": return "subagent_starting"
        case "question": return "subagent_question"
        case "blocked": return "subagent_blocked"
        case "done": return "subagent_completed"
        case "failed": return "subagent_failed"
        case "stopped", "released": return "subagent_stopped"
        case "idle": return "subagent_idle"
        default: return "subagent_unknown"
        }
    }

    private func statusColor(_ status: String, available: Bool, unreliable: Bool) -> Color {
        if !available || unreliable { return .secondary }
        switch status {
        case "working", "starting": return .cyan
        case "blocked", "question": return .orange
        case "failed": return .red
        case "done": return .green
        default: return .secondary
        }
    }
}

struct AssociatedTaskPreview: View {
    let model: ClientModel
    let host: String
    let parent: String
    let task: SubagentInfo
    @Environment(\.dismiss) private var dismiss
    @State private var history: SubagentInspectionInfo?
    @State private var failed = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading) {
                    Text(task.name).font(.headline)
                    Text([task.machine, task.provider.map { providerLabel($0) }].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                if let target = task.hostId, let session = task.sessionId {
                    Button(tr("associated_take_over")) {
                        dismiss()
                        model.command(scoped: false) { try await $0.openWorkspaceSession(hostId: target, sessionId: session) }
                    }.disabled(task.connected != true)
                }
                Button(tr("associated_task_close")) { dismiss() }
            }
            if let history {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(history.items, id: \.id) { MessageRow(model: model, message: $0, readOnly: true) }
                    }
                }
            } else if failed {
                Text(tr("subagents_unavailable")).foregroundStyle(.secondary)
            } else {
                ProgressView()
            }
        }
        .padding(20).frame(minWidth: 450, minHeight: 300)
        .task(id: task.id) {
            guard task.connected == true else { failed = true; return }
            do {
                history = try await model.inspectTask(host: host, parent: parent, child: task.id)
                failed = history == nil
            } catch { failed = true }
        }
    }
}
