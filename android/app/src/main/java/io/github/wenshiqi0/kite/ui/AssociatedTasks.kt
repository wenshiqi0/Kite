package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.SubagentInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

internal fun associatedTaskTitle(task: SubagentInfo): String = task.task.substringAfterLast('/').ifBlank { task.name }

@Composable
internal fun AssociatedTasks(
    context: String,
    tasks: List<SubagentInfo>,
    failed: Boolean,
    connected: Boolean,
    modifier: Modifier = Modifier,
    initiallyExpanded: Boolean = false,
    inspect: ((SubagentInfo) -> Unit)? = null,
) {
    if (tasks.isEmpty() && !failed) return
    var expanded by rememberSaveable(context) { mutableStateOf(initiallyExpanded) }
    // Provider ancestry remains in each record for read authorization, never UI indentation.
    val rows = remember(tasks) { tasks.distinctBy { it.id } }
    val active = rows.count { it.status in listOf("working", "starting") }
    val completed = rows.count { sessionStatus(it.status, connected) == SessionStatus.Completed }
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable { expanded = !expanded }.padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            ProductIcon(Mark.Chevron, modifier = Modifier.size(15.dp).rotate(if (expanded) 0f else -90f))
            Text(stringResource(R.string.associated_tasks_count, rows.size), color = KiteColors.Muted, fontSize = 12.sp)
            if (connected && active > 0 && !failed) Text(stringResource(R.string.associated_tasks_active, active), color = KiteColors.Accent, fontSize = 11.sp)
            if (completed > 0 && !failed) Text(stringResource(R.string.associated_tasks_completed, completed), color = KiteColors.Muted, fontSize = 11.sp)
        }
        if (expanded) {
            if (failed) Text(stringResource(R.string.associated_tasks_unavailable), color = KiteColors.Warning, fontSize = 11.sp, modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp))
            // Keep the header reachable and leave the main conversation room to read.
            val label = stringResource(R.string.associated_tasks_title)
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 96.dp).semantics { contentDescription = label }) {
                items(rows, key = { it.id }) { task ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(enabled = inspect != null) { inspect?.invoke(task) }.padding(horizontal = 12.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(associatedTaskTitle(task), fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, color = KiteColors.Text)
                            if (task.name.isNotBlank() && task.name != associatedTaskTitle(task)) Text(task.name, fontSize = 10.sp, color = KiteColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        SessionStatusBadge(sessionStatus(if (failed && task.status in listOf("working", "blocked", "question", "starting")) "unknown" else task.status, connected))
                    }
                }
            }
        }
    }
}
