package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.ui.theme.KiteColors

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*

@Composable fun providerLabel(provider: String): String = when (provider) {
    "codex" -> stringResource(R.string.provider_codex)
    "claude" -> stringResource(R.string.provider_claude)
    "dsh" -> stringResource(R.string.provider_dsh)
    "ply" -> stringResource(R.string.provider_ply)
    else -> provider
}
@Composable fun sessionTitle(session: SessionInfo): String = session.title.ifBlank { providerLabel(session.provider) + " · " + stringResource(R.string.new_session_short) }

@Composable
fun SessionDrawer(vm: HostCheckViewModel, state: CoreSnapshot?, close: () -> Unit, open: (String, String) -> Unit, create: () -> Unit, settings: () -> Unit) {
    val host = state?.selectedHost.orEmpty()
    var archived by rememberSaveable(host) { mutableStateOf(false) }
    var query by rememberSaveable(host) { mutableStateOf("") }
    var sortMenu by remember(host) { mutableStateOf(false) }
    var hostsMenu by remember { mutableStateOf(false) }
    val listState = rememberLazyListState()
    Column(Modifier.fillMaxHeight().width(310.dp)) {
        Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 12.dp, top = 16.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.app_name), fontSize = 26.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            val description = stringResource(R.string.close_navigation)
            IconButton(onClick = close, modifier = Modifier.semantics { contentDescription = description }) { ProductIcon(Mark.Close) }
        }
        Box(Modifier.padding(horizontal = 14.dp)) {
            TextButton(onClick = { hostsMenu = true }) {
                ProductIcon(Mark.Monitor, modifier = Modifier.size(18.dp)); Spacer(Modifier.width(8.dp))
                Text(state?.hosts?.find { it.id == host }?.host ?: stringResource(R.string.hosts_title), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            DropdownMenu(hostsMenu, { hostsMenu = false }) {
                state?.hosts.orEmpty().forEach { machine ->
                    val connected = state?.hostConnections?.find { it.hostId == machine.id }?.connection == ConnectionStatus.CONNECTED
                    DropdownMenuItem(text = { Text((if(machine.id == host) "✓ " else "") + machine.host + if(connected) " · " + stringResource(R.string.host_online) else "") },
                        onClick = { hostsMenu = false; vm.connectSaved(machine) }, enabled = !vm.sessionBusy)
                }
            }
        }
        TextButton(onClick = create, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.creatingAgent && !vm.sessionBusy, modifier = Modifier.padding(horizontal = 14.dp)) { ProductIcon(Mark.Plus); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.new_session)) }
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(false, true).forEach { filter ->
                FilterChip(selected = archived == filter, onClick = { archived = filter }, label = { Text(stringResource(if (filter) R.string.archived_sessions else R.string.active_sessions) + " (${state?.sessions.orEmpty().count { it.archived == filter }})") })
            }
        }
        OutlinedTextField(query, { query = it }, label = { Text(stringResource(R.string.search_sessions)) }, singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp))
        Box(Modifier.padding(horizontal = 14.dp)) {
            TextButton(onClick = { sortMenu = true }) { Text(stringResource(sessionSortLabel(state?.sessionSort ?: SessionSortOrder.NEWEST))) }
            DropdownMenu(sortMenu, { sortMenu = false }) {
                SessionSortOrder.entries.forEach { order ->
                    DropdownMenuItem(text = { Text((if(state?.sessionSort == order) "✓ " else "") + stringResource(sessionSortLabel(order))) },
                        onClick = { sortMenu = false; vm.setSessionSort(order) }, enabled = !vm.sessionBusy)
                }
            }
        }
        val sessions = remember(state?.sessions,archived,query){state?.sessions.orEmpty().filter { it.archived == archived && (query.isBlank() || it.title.contains(query,true) || it.preview.contains(query,true) || it.provider.contains(query,true)) }}
        LaunchedEffect(host, archived, state?.sessionSort) {
            val selected = sessions.indexOfFirst { it.id == state?.selectedSession }
            listState.scrollToItem(selected.coerceAtLeast(0))
        }
        if (archived) ArchivedReleaseControls(
            enabled = state?.connection == ConnectionStatus.CONNECTED && state.providers.any { it.release } && !vm.sessionBusy,
            releasing = vm.releasingArchived,
            result = vm.archiveReleaseStatus?.takeIf { vm.archiveReleaseHost == state?.selectedHost },
            onRelease = vm::releaseArchivedSessions)
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), state = listState, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(sessions, key = { "$host/${it.id}" },contentType={"session"}) { session ->
                var menu by remember(host,session.id) { mutableStateOf(false) }
                val selected = state?.selectedSession == session.id
                Surface(color = if (selected) KiteColors.AccentSurface else Color.Transparent, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                    Row(Modifier.fillMaxWidth().semantics { this.selected = selected }.clickable(enabled = !vm.sessionBusy) { open(host, session.id) }.padding(start = 12.dp, top = 10.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text(providerLabel(session.provider), fontSize = 10.sp, color = KiteColors.Muted)
                                val status = if (selected && state?.question != null) "question" else if (selected && state?.approval != null) "blocked" else session.status
                                SessionStatusBadge(sessionStatus(status, state?.connection == ConnectionStatus.CONNECTED))
                            }
                            Text(sessionTitle(session), fontSize = 14.sp, fontWeight = if (selected) FontWeight.Medium else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            val subtitle = session.preview.ifBlank { stringResource(if (session.provider == "claude" && session.agentId == null) R.string.claude_not_connected else if (session.recorded) R.string.session_recorded else R.string.session_new_hint) }
                            Text(subtitle, fontSize = 11.sp, color = KiteColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                        Box {
                            val label = stringResource(R.string.session_actions)
                            IconButton(onClick = { menu = true }, modifier = Modifier.size(40.dp).semantics { contentDescription = label }) { ProductIcon(Mark.More, modifier = Modifier.size(17.dp)) }
                            DropdownMenu(menu, { menu = false }) {
                                DropdownMenuItem(text = { Text(stringResource(if (archived) R.string.restore_session else R.string.archive_session)) }, onClick = { menu = false; vm.archiveSession(session.id, !session.archived, host) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy)
                                DropdownMenuItem(text = { Text(stringResource(R.string.delete_session), color = MaterialTheme.colorScheme.error) }, onClick = { menu = false; vm.requestSessionDeletion(session, host) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy)
                            }
                        }
                    }
                }
            }
            if (sessions.isEmpty()) item { Text(stringResource(if (archived) R.string.no_archived_sessions else R.string.no_active_sessions), fontSize = 13.sp, color = KiteColors.Muted, modifier = Modifier.padding(12.dp)) }
        }
        TextButton(onClick = settings, modifier = Modifier.padding(12.dp)) { ProductIcon(Mark.Settings); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.host_settings)) }
    }
}

internal fun sessionSortLabel(order: SessionSortOrder): Int = when(order) {
    SessionSortOrder.NEWEST -> R.string.sort_newest
    SessionSortOrder.OLDEST -> R.string.sort_oldest
    SessionSortOrder.RECENT_ACTIVITY -> R.string.sort_activity
    SessionSortOrder.TITLE -> R.string.sort_title
}

@Composable
internal fun SessionStatusBadge(status: SessionStatus) {
    val color = when (status) {
        SessionStatus.Running, SessionStatus.Starting -> KiteColors.Accent
        SessionStatus.Question, SessionStatus.Attention, SessionStatus.Paused -> KiteColors.Warning
        SessionStatus.Completed -> KiteColors.Success
        SessionStatus.Failed -> MaterialTheme.colorScheme.error
        else -> KiteColors.Muted
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.size(6.dp).background(color, RoundedCornerShape(3.dp)))
        Text(stringResource(status.label), fontSize = 10.sp, color = color)
    }
}

@Composable
internal fun ArchivedReleaseControls(enabled: Boolean, releasing: Boolean, result: String?, onRelease: () -> Unit) {
    Column {
        TextButton(onClick = onRelease, enabled = enabled && !releasing, modifier = Modifier.padding(horizontal = 14.dp)) {
            if (releasing) { CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 1.5.dp); Spacer(Modifier.width(8.dp)) }
            Text(stringResource(if (releasing) R.string.archived_releasing else R.string.release_archived_processes))
        }
        result?.let { Text(it, fontSize = 12.sp, color = KiteColors.Muted, modifier = Modifier.padding(horizontal = 24.dp, vertical = 4.dp)) }
    }
}

@Composable
internal fun DeleteSessionDialog(title: String, enabled: Boolean, onDelete: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.delete_session)) },
        text = { Text(stringResource(R.string.delete_session_confirmation, title)) },
        confirmButton = { TextButton(onClick = onDelete, enabled = enabled) { Text(stringResource(R.string.delete_session), color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}
