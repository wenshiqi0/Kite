package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.SubagentInfo
import io.github.wenshiqi0.kite.core.SubagentInspectionInfo
import io.github.wenshiqi0.kite.core.ItemRole
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.CancellationException

internal data class SubagentTarget(val host: String, val parent: String, val agent: SubagentInfo)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SubagentInspector(
    agent: SubagentInfo,
    connected: Boolean,
    load: suspend () -> SubagentInspectionInfo,
    dismiss: () -> Unit,
) {
    var history by remember { mutableStateOf<SubagentInspectionInfo?>(null) }
    var loading by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var refresh by remember { mutableIntStateOf(0) }
    val latestLoad by rememberUpdatedState(load)
    LaunchedEffect(connected, refresh) {
        if (!connected) { failed = true; loading = false; return@LaunchedEffect }
        loading = true
        failed = false
        try { history = latestLoad() }
        catch (error: CancellationException) { throw error }
        catch (_: Exception) { failed = true }
        finally { loading = false }
    }
    val entries = remember(history) { chatEntries(history?.items.orEmpty()) }
    val scroll = rememberLazyListState()
    LaunchedEffect(history) { if (entries.isNotEmpty()) scroll.scrollToItem(entries.lastIndex) }
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(.9f).padding(horizontal = 16.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(agent.name, modifier = Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                SessionStatusBadge(sessionStatus(agent.status, connected))
                val label = stringResource(R.string.dismiss)
                IconButton(onClick = dismiss, modifier = Modifier.semantics { contentDescription = label }) { ProductIcon(Mark.Close) }
            }
            if (agent.task.isNotBlank()) Text(agent.task, fontSize = 12.sp, color = KiteColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (agent.updatedMs > 0) Text(stringResource(R.string.subagent_last_update,
                java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.MEDIUM).format(java.util.Date(agent.updatedMs))), fontSize = 12.sp, color = KiteColors.Muted)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(if (history?.hasOlder == true) R.string.subagent_recent_history else R.string.subagent_readonly_history), modifier = Modifier.weight(1f), fontSize = 12.sp, color = KiteColors.Muted)
                if (loading) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 1.5.dp)
                TextButton(onClick = { refresh++ }, enabled = connected && !loading) { Text(stringResource(R.string.subagent_refresh)) }
            }
            if (failed) Text(stringResource(if (history == null) R.string.subagent_history_unavailable else R.string.subagent_refresh_failed), color = KiteColors.Warning, fontSize = 12.sp)
            if (!loading && !failed && entries.isEmpty()) Text(stringResource(R.string.subagent_history_empty), color = KiteColors.Muted)
            LazyColumn(Modifier.weight(1f).fillMaxWidth(), state = scroll, contentPadding = PaddingValues(vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                items(entries, key = { it.key }) { entry ->
                    when (entry) {
                        is ChatEntry.Work -> ToolGroupCard(entry, connected && agent.status == "working", {}, {}, desktopEnabled = false)
                        is ChatEntry.Message -> if (entry.message.role == ItemRole.ERROR) TurnFailureMessage(entry.message.text)
                            else ItemContent(entry.message, desktopEnabled = false) { file -> Text(file.name, fontSize = 13.sp, color = KiteColors.Muted) }
                    }
                }
            }
        }
    }
}
