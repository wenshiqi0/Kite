package io.github.wenshiqi0.kite.ui

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostSetupScreen(viewModel: HostCheckViewModel, onBack: () -> Unit) {
    val snapshot by viewModel.engine.state.collectAsState()
    val platformError by viewModel.engine.platformError.collectAsState()
    val state = snapshot
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.host_settings)) }, navigationIcon = {
        IconButton(onClick = onBack) { ProductIcon(Mark.Back) }
    }, actions = { state?.let { CopyDiagnosticsButton(it) } }) }) { padding ->
        Column(Modifier.padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (state == null || !state.initialized) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                Text(stringResource(R.string.core_initializing))
                if (state?.failure != null) FailureCard(state.failure!!, onForget = null)
            } else {
                val connected = state.connection == ConnectionStatus.CONNECTED
                val running = state.connection == ConnectionStatus.CONNECTING
                if (state.hosts.size > 1) Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        Text(stringResource(R.string.saved_hosts), style = MaterialTheme.typography.titleMedium)
                        state.hosts.forEach { host -> TextButton(onClick = { viewModel.chooseHost(host) }, enabled = !connected && !running) { Text("${host.username}@${host.host}:${host.port}") } }
                    }
                }
                Text(connectionStepLabel(state), color = if (state.connection == ConnectionStatus.FAILED) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
                state.failure?.let { failure -> FailureCard(failure, if (failure.kind == FailureKind.HOST_KEY_CHANGED) ({ state.selectedHost?.let(viewModel::forget) }) else null) }
                HostFormCard(viewModel, enabled = !connected && !running && state.pendingHostKey == null, running = running, connected = connected)
                state.deviceKey?.let { DeviceKeyCard(it) }
                if (running) LinearProgressIndicator(Modifier.fillMaxWidth())
                if (connected) {
                    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(stringResource(R.string.connection_active), style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(R.string.herdr_connected_version, state.herdrVersion.orEmpty()))
                            Button(onClick = viewModel::openDesktop, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.open_desktop)) }
                        }
                    }
                }
            }
            viewModel.localError?.let { Text(it, color = MaterialTheme.colorScheme.error); TextButton(onClick = viewModel::clearLocalError) { Text(stringResource(R.string.dismiss)) } }
            platformError?.let { DiagnosticCard(it) { viewModel.clearLocalError() } }

        }
    }
    state?.pendingHostKey?.let { pending ->
        AlertDialog(onDismissRequest = viewModel::disconnect,
            title = { Text(stringResource(R.string.host_key_title)) },
            text = { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { Text(stringResource(R.string.host_key_body, pending.address)); Text(pending.fingerprint, fontFamily = FontFamily.Monospace); Text(stringResource(R.string.host_key_hint)) } },
            confirmButton = { TextButton(onClick = { viewModel.trust(pending.hostId, pending.fingerprint) }) { Text(stringResource(R.string.host_key_trust)) } },
            dismissButton = { TextButton(onClick = viewModel::disconnect) { Text(stringResource(R.string.cancel)) } })
    }
}

@Composable
private fun DeviceKeyCard(key: DeviceKeyInfo) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var expanded by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.device_key_title), style = MaterialTheme.typography.titleMedium)
            Text(key.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            if (expanded) SelectionContainer { Text(key.publicKeyOpenssh, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
            TextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.device_key_details)) }
            Text(stringResource(R.string.device_key_hint), style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = { scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("ssh public key", key.publicKeyOpenssh))) } }) { Text(stringResource(R.string.device_key_copy)) }
        }
    }
}

@Composable
private fun HostFormCard(vm: HostCheckViewModel, enabled: Boolean, running: Boolean, connected: Boolean) {
    val form = vm.form
    var advanced by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.host_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.connection_network_hint), style = MaterialTheme.typography.bodySmall)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(form.host, { vm.updateForm(form.copy(host = it)) }, enabled = enabled, singleLine = true,
                    label = { Text(stringResource(R.string.host_address)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri), modifier = Modifier.weight(3f))
                OutlinedTextField(form.port, { vm.updateForm(form.copy(port = it)) }, enabled = enabled, singleLine = true,
                    label = { Text(stringResource(R.string.host_port)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.weight(1f))
            }
            OutlinedTextField(form.username, { vm.updateForm(form.copy(username = it)) }, enabled = enabled, singleLine = true,
                label = { Text(stringResource(R.string.host_username)) }, modifier = Modifier.fillMaxWidth())
            TextButton(onClick = { advanced = !advanced }) { Text(stringResource(R.string.advanced_settings)) }
            if (advanced) {
                OutlinedTextField(form.herdrPath, { vm.updateForm(form.copy(herdrPath = it)) }, enabled = enabled, singleLine = true,
                    label = { Text(stringResource(R.string.host_herdr_path)) }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(form.streamHost, { vm.updateForm(form.copy(streamHost = it)) }, enabled = enabled, singleLine = true,
                    label = { Text(stringResource(R.string.stream_address)) }, supportingText = { Text(stringResource(R.string.stream_address_hint)) }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(form.sunshinePort, { vm.updateForm(form.copy(sunshinePort = it)) }, enabled = enabled, singleLine = true,
                    label = { Text(stringResource(R.string.sunshine_port)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
            }
            if (connected || running) OutlinedButton(onClick = vm::disconnect, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.disconnect)) }
            else Button(onClick = vm::connect, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.connect_herdr)) }
        }
    }
}

@Composable
private fun FailureCard(failure: FailureInfo, onForget: (() -> Unit)?) {
    val title = when (failure.kind) {
        FailureKind.AUTHENTICATION -> stringResource(R.string.error_auth_rejected, failure.username)
        FailureKind.HOST_KEY_CHANGED -> stringResource(R.string.error_host_key_changed, failure.trustedFingerprint, failure.presentedFingerprint)
        FailureKind.HERDR_MISSING -> stringResource(R.string.error_herdr_not_found)
        FailureKind.HERDR_UNSUPPORTED -> stringResource(R.string.error_herdr_capability)
        FailureKind.HERDR_API -> stringResource(R.string.error_herdr_api)
        FailureKind.STORAGE, FailureKind.INVALID_KEY -> stringResource(R.string.error_secure_storage)
        FailureKind.STREAM -> stringResource(R.string.error_desktop)
        FailureKind.CONNECTION -> stringResource(when {
            failure.details.contains("Connection refused", ignoreCase = true) -> R.string.error_connection_refused
            failure.details.contains("timed out", ignoreCase = true) -> R.string.error_connection_timeout
            failure.details.contains("lookup", ignoreCase = true) || failure.details.contains("resolve", ignoreCase = true) -> R.string.error_connection_dns
            else -> R.string.error_connection_generic
        })
        FailureKind.AGENT_START -> stringResource(R.string.agent_start_failed)
        FailureKind.INVALID_CONFIGURATION -> stringResource(R.string.error_action)
    }
    var details by remember(failure.details) { mutableStateOf(true) }
    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer)) {
        Column(Modifier.padding(16.dp)) {
            Text(title)
            TextButton(onClick = { details = !details }) { Text(stringResource(R.string.details)) }
            if (details) SelectionContainer { Text(failure.details, style = MaterialTheme.typography.bodySmall) }
            onForget?.let { TextButton(onClick = it) { Text(stringResource(R.string.forget_host_key)) } }
        }
    }
}

@Composable
private fun DiagnosticCard(details: String, dismiss: () -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp)) {
        Text(stringResource(R.string.error_action))
        TextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.details)) }
        if (expanded) Text(details)
        TextButton(onClick = dismiss) { Text(stringResource(R.string.dismiss)) }
    } }
}

@Composable internal fun connectionStepLabel(state: CoreSnapshot): String = stringResource(when (state.connection) {
    ConnectionStatus.CONNECTED -> if (state.connectionStep == ConnectionStep.AGENT) R.string.connection_agent else R.string.connection_ready
    ConnectionStatus.FAILED -> R.string.connection_failed_hint
    ConnectionStatus.AWAITING_TRUST -> R.string.host_key_title
    ConnectionStatus.DISCONNECTED -> R.string.workspace_connect_host
    ConnectionStatus.CONNECTING -> when (state.connectionStep) {
        ConnectionStep.HERDR_DISCOVERY -> R.string.connection_discovery
        ConnectionStep.HERDR_API -> R.string.connection_api
        ConnectionStep.AGENT -> R.string.connection_agent
        else -> R.string.connection_ssh
    }
})

internal fun connectionDiagnostics(state: CoreSnapshot): String = buildString {
    val host = state.hosts.find { it.id == state.selectedHost }
    append("host=${host?.username}@${host?.host}:${host?.port}\n")
    append("connection=${state.connection} step=${state.connectionStep}\n")
    append("device=${state.deviceKey?.fingerprint}\nherdr=${state.herdrVersion} path=${state.herdrPath}\n")
    append("agents=${state.agents.size} selected=${state.selectedAgent}\n")
    state.agents.find { it.id == state.selectedAgent }?.let { append("agent=${it.status} starting=${it.launchPending} managed_ready=${it.managedReady}\n") }
    state.failure?.let { append("error=${it.kind}: ${it.details}") }
}

@Composable private fun CopyDiagnosticsButton(state: CoreSnapshot) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    TextButton(onClick = { scope.launch { clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Kite connection diagnostics", connectionDiagnostics(state)))) } }) { Text(stringResource(R.string.copy_diagnostics)) }
}
