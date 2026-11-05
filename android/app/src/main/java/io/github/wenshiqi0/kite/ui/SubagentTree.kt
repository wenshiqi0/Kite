package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.material3.IconButton
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.SubagentInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

internal data class SubagentRow(val agent: SubagentInfo, val branches: List<Boolean>, val hasChildren: Boolean)

/** Parent IDs, rather than task path spelling or list order, define the tree. */
internal fun subagentRows(agents: List<SubagentInfo>, collapsed: Set<String>): List<SubagentRow> {
    val unique = agents.distinctBy { it.id }
    val ids = unique.map { it.id }.toSet()
    val children = unique.groupBy { it.parentId }
    val seen = mutableSetOf<String>()
    val rows = mutableListOf<SubagentRow>()
    fun visit(siblings: List<SubagentInfo>, ancestors: List<Boolean>) {
        if (ancestors.size > 32) return
        siblings.forEachIndexed { index, agent ->
            if (seen.add(agent.id)) {
                val branches = ancestors + (index < siblings.lastIndex)
                rows += SubagentRow(agent, branches, !children[agent.id].isNullOrEmpty())
                if (agent.id !in collapsed) visit(children[agent.id].orEmpty(), branches)
            }
        }
    }
    visit(unique.filter { it.parentId !in ids }, emptyList())
    return rows
}

@Composable
internal fun SubagentTree(context: String, agents: List<SubagentInfo>, failed: Boolean, connected: Boolean,
    modifier: Modifier = Modifier, initiallyExpanded: Boolean = false,
    inspect: ((SubagentInfo) -> Unit)? = null) {
    if (agents.isEmpty() && !failed) return
    var expanded by rememberSaveable(context) { mutableStateOf(initiallyExpanded) }
    var collapsed by rememberSaveable(context) { mutableStateOf(emptyList<String>()) }
    val rows = remember(agents, collapsed) { subagentRows(agents, collapsed.toSet()) }
    val active = agents.count { it.status in listOf("working", "starting") }
    val completed = agents.count { sessionStatus(it.status, connected) == SessionStatus.Completed }
    Column(modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth().heightIn(min = 40.dp).clickable { expanded = !expanded }.padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ProductIcon(Mark.Chevron, modifier = Modifier.size(15.dp).rotate(if (expanded) 0f else -90f))
            Text(stringResource(R.string.subagents_count, agents.size), color = KiteColors.Muted, fontSize = 12.sp)
            if (connected && active > 0 && !failed) Text(stringResource(R.string.subagents_active, active), color = KiteColors.Accent, fontSize = 11.sp)
            if (completed > 0 && !failed) Text(stringResource(R.string.subagents_completed, completed), color = KiteColors.Muted, fontSize = 11.sp)
        }
        if (expanded) {
            if (failed) Text(stringResource(R.string.subagents_unavailable), color = KiteColors.Warning, fontSize = 11.sp, modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp))
            rows.forEach { row -> key(row.agent.id) {
                fun toggle() { collapsed = if (row.agent.id in collapsed) collapsed - row.agent.id else collapsed + row.agent.id }
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(enabled = inspect != null || row.hasChildren) {
                    if (inspect != null) inspect(row.agent) else toggle()
                }.padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    val lineColor = KiteColors.Muted.copy(alpha = .35f)
                    Canvas(Modifier.width((row.branches.size * 12).dp).height(44.dp)) {
                        row.branches.forEachIndexed { index, continues ->
                            val x = (index * 12 + 6).dp.toPx()
                            if (index == row.branches.lastIndex) {
                                drawLine(lineColor, Offset(x, 0f), Offset(x, if (continues) size.height else size.height / 2), 1.dp.toPx())
                                drawLine(lineColor, Offset(x, size.height / 2), Offset(size.width, size.height / 2), 1.dp.toPx())
                            } else if (continues) drawLine(lineColor, Offset(x, 0f), Offset(x, size.height), 1.dp.toPx())
                        }
                    }
                    if (row.hasChildren) {
                        val label = stringResource(if (row.agent.id in collapsed) R.string.subagent_expand else R.string.subagent_collapse, row.agent.name)
                        IconButton(onClick = ::toggle, modifier = Modifier.size(40.dp).semantics { contentDescription = label }) {
                            ProductIcon(Mark.Chevron, modifier = Modifier.size(14.dp).rotate(if (row.agent.id in collapsed) -90f else 0f))
                        }
                    }
                    else Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f).padding(horizontal = 5.dp)) {
                        Text(row.agent.name, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, color = KiteColors.Text)
                        if (row.agent.task.isNotBlank()) Text(row.agent.task.substringAfterLast('/'), fontSize = 10.sp, color = KiteColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    SessionStatusBadge(sessionStatus(if (failed && row.agent.status in listOf("working", "blocked", "question", "starting")) "unknown" else row.agent.status, connected))
                }
            } }
        }
    }
}
