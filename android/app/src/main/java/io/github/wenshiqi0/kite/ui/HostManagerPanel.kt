package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostSetupScreen(vm: HostCheckViewModel, onBack: () -> Unit) {
    if (vm.hostEditorVisible) { HostEditorScreen(vm) { vm.hostEditorVisible = false }; return }
    val state by vm.engine.state.collectAsState()
    val platformError by vm.engine.platformError.collectAsState()
    var keyVisible by remember { mutableStateOf(false) }
    var settings by remember { mutableStateOf(false) }
    val backLabel = stringResource(R.string.host_back)
    val settingsLabel = stringResource(R.string.connection_settings)
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.hosts_title)) }, navigationIcon = {
            IconButton(onClick = onBack, modifier = Modifier.semantics { contentDescription = backLabel }) { ProductIcon(Mark.Back) }
        }, actions = {
            IconButton(onClick = { settings = true }, modifier = Modifier.semantics { contentDescription = settingsLabel }) { ProductIcon(Mark.Settings) }
            TextButton(onClick = vm::newHost, enabled = !vm.savingHost && state?.connection != ConnectionStatus.CONNECTING) {
                ProductIcon(Mark.Plus, modifier = Modifier.size(18.dp)); Spacer(Modifier.width(4.dp)); Text(stringResource(R.string.add_host))
            }
        })
    }) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            items(state?.hosts.orEmpty(), key = { it.id }, contentType = { "host" }) { host ->
                val status = if (state?.selectedHost == host.id) state?.connection ?: ConnectionStatus.DISCONNECTED else ConnectionStatus.DISCONNECTED
                HostConnectionCard(host, status, !vm.savingHost,
                    activate = { if (status == ConnectionStatus.CONNECTED) onBack() else vm.connectSaved(host) },
                    edit = { vm.chooseHost(host) })
            }
            if (state?.hosts.isNullOrEmpty()) item {
                Column(Modifier.fillMaxWidth().padding(vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    ProductIcon(Mark.Monitor, modifier = Modifier.size(32.dp))
                    Spacer(Modifier.height(12.dp)); Text(stringResource(R.string.hosts_empty))
                    TextButton(onClick = vm::newHost) { Text(stringResource(R.string.add_host)) }
                }
            }
            if (vm.localError != null || platformError != null) item {
                TextButton(onClick = { settings = true }) { Text(stringResource(R.string.host_operation_issue)) }
            }
        }
    }
    if (settings) ModalBottomSheet(onDismissRequest = { settings = false }) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(settingsLabel, style = MaterialTheme.typography.titleMedium)
            ClipboardSyncSetting(vm)
            TextButton(onClick = { settings = false; keyVisible = true }) { Text(stringResource(R.string.device_key_title)) }
            state?.failure?.let { failure ->
                FailureCard(failure, if (failure.kind == FailureKind.HOST_KEY_CHANGED) ({ state?.selectedHost?.let(vm::forget) }) else null)
            }
            vm.localError?.let { DiagnosticCard(it, vm::clearLocalError) }
            platformError?.let { DiagnosticCard(it, vm::clearLocalError) }
            state?.let { CopyDiagnosticsButton(it) }
        }
    }
    if (keyVisible) state?.deviceKey?.let { key ->
        AlertDialog(onDismissRequest = { keyVisible = false }, title = { Text(stringResource(R.string.device_key_title)) },
            text = { DeviceKeyCard(key) }, confirmButton = { TextButton(onClick = { keyVisible = false }) { Text(stringResource(R.string.dismiss)) } })
    }
    state?.pendingHostKey?.let { pending ->
        AlertDialog(onDismissRequest = vm::disconnect, title = { Text(stringResource(R.string.host_key_title)) },
            text = { Column { Text(pending.address); Text(pending.fingerprint, fontFamily = FontFamily.Monospace); Text(stringResource(R.string.host_key_hint)) } },
            confirmButton = { TextButton(onClick = { vm.trust(pending.hostId, pending.fingerprint) }) { Text(stringResource(R.string.host_key_trust)) } },
            dismissButton = { TextButton(onClick = vm::disconnect) { Text(stringResource(R.string.cancel)) } })
    }
}

/** Connection facts come from Core. An unconnected profile is never labelled unreachable. */
@Composable
internal fun HostConnectionCard(host: HostRecord, status: ConnectionStatus, enabled: Boolean, activate: () -> Unit, edit: () -> Unit) {
    val connected = status == ConnectionStatus.CONNECTED
    val busy = status == ConnectionStatus.CONNECTING || status == ConnectionStatus.AWAITING_TRUST
    val statusLabel = stringResource(when (status) {
        ConnectionStatus.CONNECTED -> R.string.host_online
        ConnectionStatus.CONNECTING -> R.string.host_connecting
        ConnectionStatus.AWAITING_TRUST -> R.string.host_confirm
        ConnectionStatus.FAILED -> R.string.host_failed
        ConnectionStatus.DISCONNECTED -> R.string.host_not_connected
    })
    val color = when (status) {
        ConnectionStatus.CONNECTED -> KiteColors.Success
        ConnectionStatus.FAILED -> MaterialTheme.colorScheme.error
        ConnectionStatus.CONNECTING, ConnectionStatus.AWAITING_TRUST -> KiteColors.Warning
        ConnectionStatus.DISCONNECTED -> KiteColors.Muted
    }
    val editLabel = stringResource(R.string.host_edit_named, host.host)
    val openLabel = stringResource(R.string.host_open_named, host.host)
    Card(onClick = activate, enabled = enabled && !busy, modifier = Modifier.fillMaxWidth().semantics { contentDescription = openLabel },
        colors = CardDefaults.cardColors(containerColor = if (connected) KiteColors.AccentSurface else KiteColors.Surface,
            contentColor = KiteColors.Text, disabledContainerColor = KiteColors.Surface, disabledContentColor = KiteColors.Text)) {
        Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 4.dp, top = 10.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(36.dp).background(KiteColors.Raised, CircleShape), contentAlignment = Alignment.Center) { ProductIcon(Mark.Monitor, modifier = Modifier.size(19.dp)) }
            Column(Modifier.weight(1f).padding(start = 12.dp, end = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(host.host, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (status == ConnectionStatus.CONNECTING) CircularProgressIndicator(Modifier.size(9.dp), strokeWidth = 1.5.dp, color = color)
                    else Box(Modifier.size(6.dp).background(color, CircleShape))
                    Text(statusLabel, fontSize = 12.sp, color = color)
                }
            }
            TextButton(onClick = activate, enabled = enabled && !busy, contentPadding = PaddingValues(horizontal = 8.dp)) {
                Text(stringResource(if (connected) R.string.host_enter else if (status == ConnectionStatus.FAILED) R.string.retry else R.string.host_connect), fontSize = 13.sp)
            }
            IconButton(onClick = edit, enabled = enabled && !busy, modifier = Modifier.size(48.dp).semantics { contentDescription = editLabel }) { ProductIcon(Mark.More, modifier = Modifier.size(18.dp)) }
        }
    }
}
