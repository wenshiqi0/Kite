package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*

@Composable fun providerLabel(provider: String) = stringResource(if (provider == "claude") R.string.provider_claude else R.string.provider_codex)
@Composable fun sessionTitle(session: SessionInfo): String = session.title.ifBlank { providerLabel(session.provider) + " · " + stringResource(R.string.new_session_short) }

@Composable
fun SessionDrawer(vm: HostCheckViewModel, state: CoreSnapshot?, close: () -> Unit, open: (String) -> Unit, create: () -> Unit, settings: () -> Unit) {
    var archived by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxHeight().width(310.dp)) {
        Row(Modifier.fillMaxWidth().padding(start = 24.dp, end = 12.dp, top = 16.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.app_name), fontSize = 26.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            val description = stringResource(R.string.close_navigation)
            IconButton(onClick = close, modifier = Modifier.semantics { contentDescription = description }) { ProductIcon(Mark.Close) }
        }
        TextButton(onClick = create, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.creatingAgent, modifier = Modifier.padding(horizontal = 14.dp)) { ProductIcon(Mark.Plus); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.new_session)) }
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(false, true).forEach { filter ->
                FilterChip(selected = archived == filter, onClick = { archived = filter }, label = { Text(stringResource(if (filter) R.string.archived_sessions else R.string.active_sessions)) })
            }
        }
        val sessions = state?.sessions.orEmpty().filter { it.archived == archived }
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(sessions, key = { it.id }) { session ->
                var menu by remember(session.id) { mutableStateOf(false) }
                val selected = state?.selectedSession == session.id
                Surface(color = if (selected) Color(0xFFE8F1FC) else Color.Transparent, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                    Row(Modifier.fillMaxWidth().clickable { open(session.id) }.padding(start = 12.dp, top = 10.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(providerLabel(session.provider), fontSize = 10.sp, color = Color(0xFF888982))
                            Text(sessionTitle(session), fontSize = 14.sp, fontWeight = if (selected) FontWeight.Medium else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            val subtitle = session.preview.ifBlank { stringResource(if (session.provider == "claude" && session.agentId == null) R.string.claude_not_connected else if (session.recorded) R.string.session_recorded else R.string.session_new_hint) }
                            Text(subtitle, fontSize = 11.sp, color = Color(0xFF888982), maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                        Box {
                            val label = stringResource(R.string.session_actions)
                            IconButton(onClick = { menu = true }, modifier = Modifier.size(40.dp).semantics { contentDescription = label }) { ProductIcon(Mark.More, modifier = Modifier.size(17.dp)) }
                            DropdownMenu(menu, { menu = false }) {
                                DropdownMenuItem(text = { Text(stringResource(if (archived) R.string.restore_session else R.string.archive_session)) }, onClick = { menu = false; vm.archiveSession(session.id, !session.archived) }, enabled = state?.connection == ConnectionStatus.CONNECTED && !vm.sessionBusy)
                            }
                        }
                    }
                }
            }
            if (sessions.isEmpty()) item { Text(stringResource(if (archived) R.string.no_archived_sessions else R.string.no_active_sessions), fontSize = 13.sp, color = Color(0xFF888982), modifier = Modifier.padding(12.dp)) }
        }
        TextButton(onClick = settings, modifier = Modifier.padding(12.dp)) { ProductIcon(Mark.Settings); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.host_settings)) }
    }
}
