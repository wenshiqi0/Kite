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
fun SessionDrawer(vm: HostCheckViewModel, state: CoreSnapshot?, close: () -> Unit, open: (String, String) -> Unit, create: () -> Unit, settings: () -> Unit,
    inspectSubagent: ((String, String, SubagentInfo) -> Unit)? = null) {
    var archived by rememberSaveable { mutableStateOf(false) }
    var query by rememberSaveable { mutableStateOf("") }
    var sortMenu by remember { mutableStateOf(false) }
    val collapsed = remember { mutableStateMapOf<String, Boolean>() }
    val workspaces = state?.hostConnections.orEmpty().associateBy { it.hostId }
    val allSessions = state?.hostConnections.orEmpty().flatMap { it.sessions }
    val groups = state?.hosts.orEmpty().mapNotNull { host ->
        val sessions = workspaces[host.id]?.sessions.orEmpty().filter {
            it.archived == archived && (query.isBlank() || host.host.contains(query, true) || it.title.contains(query, true) || it.preview.contains(query, true) || it.provider.contains(query, true) || it.subagents.any { child -> child.name.contains(query, true) || child.task.contains(query, true) })
        }
        if (query.isNotBlank() && sessions.isEmpty()) null else host to sessions
    }
    Column(Modifier.fillMaxHeight().width(310.dp)) {
        Row(Modifier.fillMaxWidth().height(44.dp).padding(start = 16.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.sessions_title), fontSize = 18.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            val description = stringResource(R.string.close_navigation)
            IconButton(onClick = close, modifier = Modifier.semantics { contentDescription = description }) { ProductIcon(Mark.Close) }
        }
        SessionSearchRow(query, { query = it }, create,
            !vm.creatingAgent && !vm.sessionBusy && state?.hostConnections?.any { it.connection == ConnectionStatus.CONNECTED } == true)
        Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            listOf(false, true).forEach { filter ->
                TextButton(onClick = { archived = filter }, contentPadding = PaddingValues(horizontal = 8.dp)) {
                    Text(stringResource(if (filter) R.string.active_archive_label else R.string.active_session_label) + " ${allSessions.count { it.archived == filter }}",
                        color = if (archived == filter) KiteColors.Accent else KiteColors.Muted, fontSize = 12.sp)
                }
            }
            Spacer(Modifier.weight(1f))
            Box {
                val label = stringResource(R.string.session_sort)
                IconButton(onClick = { sortMenu = true }, modifier = Modifier.semantics { contentDescription = label }) { ProductIcon(Mark.Sort, modifier = Modifier.size(18.dp)) }
                DropdownMenu(sortMenu, { sortMenu = false }) {
                    SessionSortOrder.entries.forEach { order ->
                        DropdownMenuItem(text = { Text((if(state?.sessionSort == order) "✓ " else "") + stringResource(sessionSortLabel(order))) },
                            onClick = { sortMenu = false; vm.setSessionSort(order) }, enabled = !vm.sessionBusy)
                    }
                }
            }
        }
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            groups.forEach { (host, sessions) ->
                val workspace = workspaces[host.id]
                val connected = workspace?.connection == ConnectionStatus.CONNECTED
                val expanded = query.isNotBlank() || collapsed[host.id] != true
                item(key = "machine/${host.id}", contentType = "machine") {
                    MachineSessionHeader(host, workspace?.connection ?: ConnectionStatus.DISCONNECTED, sessions.size, expanded,
                        toggle = { collapsed[host.id] = expanded },
                        configure = { vm.chooseHost(host); vm.settingsVisible = true; close() },
                        connect = { vm.connectSaved(host) }, disconnect = { vm.disconnectHost(host.id) },
                        release = if (archived && connected && workspace?.providers?.any { it.release } == true) ({ vm.releaseArchivedSessions(host.id) }) else null)
                }
                if (expanded) {
                    items(sessions, key = { "session/${host.id}/${it.id}" }, contentType = { "session" }) { session ->
                        var menu by remember(host.id, session.id) { mutableStateOf(false) }
                        val selected = state?.selectedHost == host.id && state.selectedSession == session.id
                        Surface(color = if (selected) KiteColors.AccentSurface else Color.Transparent, shape = RoundedCornerShape(9.dp), modifier = Modifier.fillMaxWidth()) {
                            Row(Modifier.fillMaxWidth().semantics { this.selected = selected }.clickable(enabled = !vm.sessionBusy) { open(host.id, session.id) }.padding(start = 12.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                                        Text(providerLabel(session.provider), fontSize = 10.sp, color = KiteColors.Muted)
                                        val status = if (selected && state?.question != null) "question" else if (selected && state?.approval != null) "blocked" else session.status
                                        SessionStatusBadge(sessionStatus(status, connected))
                                    }
                                    Text(sessionTitle(session), fontSize = 14.sp, fontWeight = if (selected) FontWeight.Medium else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    if (session.preview.isNotBlank()) Text(session.preview, fontSize = 11.sp, color = KiteColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                                Box {
                                    val label = stringResource(R.string.session_actions)
                                    IconButton(onClick = { menu = true }, modifier = Modifier.size(40.dp).semantics { contentDescription = label }) { ProductIcon(Mark.More, modifier = Modifier.size(17.dp)) }
                                    DropdownMenu(menu, { menu = false }) {
                                        DropdownMenuItem(text = { Text(stringResource(if (session.archived) R.string.restore_session else R.string.archive_session)) }, onClick = { menu = false; vm.archiveSession(session.id, !session.archived, host.id) }, enabled = connected && !vm.sessionBusy)
                                        DropdownMenuItem(text = { Text(stringResource(R.string.delete_session), color = MaterialTheme.colorScheme.error) }, onClick = { menu = false; vm.requestSessionDeletion(session, host.id) }, enabled = connected && !vm.sessionBusy)
                                    }
                                }
                            }
                        }
                        AssociatedTasks("${host.id}/${session.id}/${query.isNotBlank()}", session.subagents, session.subagentsError, connected,
                            Modifier.padding(start = 16.dp, bottom = 4.dp), initiallyExpanded = query.isNotBlank(),
                            inspect = inspectSubagent?.let { inspect -> { child -> inspect(host.id, session.id, child) } })
                    }
                    if (sessions.isEmpty()) item(key = "empty/${host.id}") { Text(stringResource(if (archived) R.string.no_archived_sessions else R.string.no_active_sessions), fontSize = 12.sp, color = KiteColors.Muted, modifier = Modifier.padding(start = 12.dp, bottom = 8.dp)) }
                }
            }
            if (groups.isEmpty()) item { Text(stringResource(if (query.isNotBlank()) R.string.no_session_matches else R.string.hosts_empty), color = KiteColors.Muted, fontSize = 13.sp, modifier = Modifier.padding(12.dp)) }
        }
        TextButton(onClick = settings, modifier = Modifier.padding(horizontal = 10.dp)) { ProductIcon(Mark.Plus, modifier = Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_host)) }
    }
}

/** A 40dp visual field keeps a 48dp touch row; creation shares its row. */
@Composable
internal fun SessionSearchRow(query: String, change: (String) -> Unit, create: () -> Unit, canCreate: Boolean) {
    val searchLabel = stringResource(R.string.search_sessions)
    val createLabel = stringResource(R.string.new_session)
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp).height(48.dp), verticalAlignment = Alignment.CenterVertically) {
        Surface(Modifier.weight(1f).height(40.dp), color = KiteColors.Raised, shape = RoundedCornerShape(8.dp)) {
            Row(Modifier.padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ProductIcon(Mark.Search, modifier = Modifier.size(17.dp))
                androidx.compose.foundation.text.BasicTextField(query, change, singleLine = true,
                    textStyle = LocalTextStyle.current.copy(color = KiteColors.Text, fontSize = 13.sp),
                    cursorBrush = androidx.compose.ui.graphics.SolidColor(KiteColors.Accent),
                    modifier = Modifier.weight(1f).semantics { contentDescription = searchLabel },
                    decorationBox = { field -> Box(contentAlignment = Alignment.CenterStart) { if(query.isEmpty())Text(searchLabel, fontSize = 13.sp, color = KiteColors.Muted); field() } })
            }
        }
        IconButton(onClick = create, enabled = canCreate, modifier = Modifier.size(48.dp).semantics { contentDescription = createLabel }) { ProductIcon(Mark.Plus, color = if(canCreate)KiteColors.Accent else KiteColors.Muted) }
    }
}

@Composable
internal fun MachineSessionHeader(host: HostRecord, connection: ConnectionStatus, count: Int, expanded: Boolean,
    toggle: () -> Unit, configure: () -> Unit, connect: () -> Unit, disconnect: () -> Unit, release: (() -> Unit)? = null) {
    var menu by remember(host.id) { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().clickable(onClick = toggle).padding(start = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        ProductIcon(Mark.Monitor, modifier = Modifier.size(15.dp))
        val color = when(connection) { ConnectionStatus.CONNECTED -> KiteColors.Success; ConnectionStatus.FAILED -> MaterialTheme.colorScheme.error; ConnectionStatus.CONNECTING, ConnectionStatus.RECONNECTING, ConnectionStatus.AWAITING_TRUST -> KiteColors.Warning; else -> KiteColors.Muted }
        Box(Modifier.padding(horizontal = 7.dp).size(5.dp).background(color, RoundedCornerShape(3.dp)))
        Text(host.host, fontSize = 12.sp, color = KiteColors.Muted, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Text(count.toString(), color = KiteColors.Muted, fontSize = 11.sp)
        Box {
            val description = stringResource(R.string.machine_more_named, host.host)
            IconButton(onClick = { menu = true }, modifier = Modifier.size(40.dp).semantics { contentDescription = description }) { ProductIcon(Mark.More, modifier = Modifier.size(17.dp)) }
            DropdownMenu(menu, { menu = false }) {
                DropdownMenuItem(text = { Text(stringResource(R.string.machine_settings)) }, onClick = { menu = false; configure() })
                DropdownMenuItem(text = { Text(stringResource(if(connection == ConnectionStatus.CONNECTED) R.string.disconnect else R.string.host_connect)) }, onClick = { menu = false; if(connection == ConnectionStatus.CONNECTED)disconnect()else connect() })
                release?.let { action -> DropdownMenuItem(text = { Text(stringResource(R.string.release_archived_processes)) }, onClick = { menu = false; action() }) }
            }
        }
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
