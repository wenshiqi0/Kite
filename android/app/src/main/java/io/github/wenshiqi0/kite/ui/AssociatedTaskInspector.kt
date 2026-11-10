package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.Velocity
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.SubagentInfo
import io.github.wenshiqi0.kite.core.SubagentInspectionInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.CancellationException

internal data class AssociatedTaskTarget(val host: String, val parent: String, val task: SubagentInfo)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AssociatedTaskInspector(
    task: SubagentInfo,
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
    var positioned by remember { mutableStateOf(false) }
    LaunchedEffect(history) {
        if (!positioned && entries.isNotEmpty()) {
            scroll.scrollToItem(entries.lastIndex)
            positioned = true
        }
    }
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val historyFlingBoundary = remember(sheetState) {
        object : NestedScrollConnection {
            override suspend fun onPostFling(consumed: Velocity, available: Velocity): Velocity =
                // Inner tool lists and history consume velocity first. At the upper sheet
                // anchor, the remaining upward velocity has nowhere to go; do not feed it
                // into another sheet spring. Downward scrolling and swipe-to-dismiss pass.
                if (available.y < 0 && sheetState.currentValue == SheetValue.Expanded && sheetState.targetValue == SheetValue.Expanded)
                    Velocity(0f, available.y) else Velocity.Zero
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        // Derive a fixed content height from the workspace. Constraining the sheet's
        // surface would also shrink the full-height constraint used for its anchors.
        // Keep bottom safe padding inside this height; offset-dependent top insets
        // must not resize the surface and restart settlement during a fast fling.
        val sheetHeight = (maxHeight * .68f).coerceIn(minOf(320.dp, maxHeight), minOf(560.dp, maxHeight))
        ModalBottomSheet(
            onDismissRequest = dismiss,
            sheetState = sheetState,
            contentWindowInsets = { WindowInsets(0, 0, 0, 0) },
        ) {
            Column(Modifier.fillMaxWidth().height(sheetHeight - 48.dp)
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom))
                .nestedScroll(historyFlingBoundary).padding(horizontal = 16.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.associated_tasks_title), fontSize = 11.sp, color = KiteColors.Muted)
                        Text(associatedTaskTitle(task), style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                    SessionStatusBadge(sessionStatus(task.status, connected))
                    val label = stringResource(R.string.dismiss)
                    IconButton(onClick = dismiss, modifier = Modifier.semantics { contentDescription = label }) { ProductIcon(Mark.Close) }
                }
                if (task.name.isNotBlank()) Text(stringResource(R.string.associated_task_executor, task.name), fontSize = 12.sp, color = KiteColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (task.updatedMs > 0) Text(stringResource(R.string.associated_task_last_update,
                    java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.MEDIUM).format(java.util.Date(task.updatedMs))), fontSize = 12.sp, color = KiteColors.Muted)
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(if (history?.hasOlder == true) R.string.associated_task_recent_history else R.string.associated_task_readonly_history), modifier = Modifier.weight(1f), fontSize = 12.sp, color = KiteColors.Muted)
                    if (loading) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 1.5.dp)
                    TextButton(onClick = { refresh++ }, enabled = connected && !loading) { Text(stringResource(R.string.associated_task_refresh)) }
                }
                if (failed) Text(stringResource(if (history == null) R.string.associated_task_history_unavailable else R.string.associated_task_refresh_failed), color = KiteColors.Warning, fontSize = 12.sp)
                if (!loading && !failed && entries.isEmpty()) Text(stringResource(R.string.associated_task_history_empty), color = KiteColors.Muted)
                val historyLabel = stringResource(R.string.associated_task_readonly_history)
                LazyColumn(Modifier.weight(1f).fillMaxWidth().semantics { contentDescription = historyLabel }, state = scroll, contentPadding = PaddingValues(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(entries, key = { it.key }) { entry ->
                        ChatEntryContent(entry, working = connected && task.status == "working", desktopEnabled = false) { file ->
                            Text(file.name, fontSize = 13.sp, color = KiteColors.Muted)
                        }
                    }
                }
            }
        }
    }
}
