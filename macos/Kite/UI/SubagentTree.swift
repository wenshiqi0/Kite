import SwiftUI

private struct SubagentNode: Identifiable {
    let agent: SubagentInfo
    let children: [SubagentNode]?
    var id: String { agent.id }

    static func roots(_ agents: [SubagentInfo]) -> [SubagentNode] {
        let ids = Set(agents.map(\.id))
        let children = Dictionary(grouping: agents, by: \.parentId)
        func node(_ agent: SubagentInfo, seen: Set<String>) -> SubagentNode {
            let descendants = seen.count >= 32 ? [] : (children[agent.id] ?? []).filter { !seen.contains($0.id) }
            let nodes = descendants.map { node($0, seen: seen.union([$0.id])) }
            return SubagentNode(agent: agent, children: nodes.isEmpty ? nil : nodes)
        }
        return agents.filter { !ids.contains($0.parentId) }.map { node($0, seen: [$0.id]) }
    }
}

struct SubagentTree: View {
    let agents: [SubagentInfo]
    let failed: Bool
    let connected: Bool
    @State private var expanded = false

    var body: some View {
        if !agents.isEmpty || failed {
            DisclosureGroup(isExpanded: $expanded) {
                if failed { Text(tr("subagents_unavailable")).foregroundStyle(.orange).font(.caption) }
                OutlineGroup(SubagentNode.roots(agents), children: \.children) { node in
                    HStack(spacing: 6) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(node.agent.name).font(.caption).lineLimit(1)
                            if !node.agent.task.isEmpty { Text(node.agent.task.split(separator: "/").last.map(String.init) ?? "").font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
                        }
                        Spacer(minLength: 4)
                        Text(tr(statusKey(node.agent.status))).font(.caption2).foregroundStyle(statusColor(node.agent.status))
                    }.padding(.vertical, 3)
                }
            } label: {
                Text(String(format: tr("subagents_count"), agents.count)).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private func statusKey(_ status: String) -> String {
        if !connected { return "subagent_disconnected" }
        if failed && ["working", "starting", "blocked", "question"].contains(status) { return "subagent_unknown" }
        switch status {
        case "working": return "subagent_running"
        case "starting": return "subagent_starting"
        case "question": return "subagent_question"
        case "blocked": return "subagent_blocked"
        case "done": return "subagent_completed"
        case "failed": return "subagent_failed"
        case "stopped": return "subagent_stopped"
        case "idle": return "subagent_idle"
        default: return "subagent_unknown"
        }
    }
    private func statusColor(_ status: String) -> Color {
        if !connected || failed { return .secondary }
        switch status {
        case "working", "starting": return .cyan
        case "blocked", "question": return .orange
        case "failed": return .red
        case "done": return .green
        default: return .secondary
        }
    }
}
