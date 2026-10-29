package io.github.wenshiqi0.kite.ui

import android.content.ClipData
import androidx.activity.compose.BackHandler
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
import androidx.compose.ui.viewinterop.AndroidView
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.media.VideoSurface
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostCheckScreen(viewModel: HostCheckViewModel) {
    val snapshot by viewModel.engine.state.collectAsState()
    val platformError by viewModel.engine.platformError.collectAsState()
    val state = snapshot
    BackHandler(enabled = viewModel.showDesktop) { viewModel.closeDesktop() }
    if (state != null && viewModel.showDesktop) {
        DesktopScreen(viewModel, state)
    } else Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.app_name)) }) }) { padding ->
        Column(Modifier.padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            state?.deviceKey?.let { DeviceKeyCard(it) }
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
                HostFormCard(viewModel, enabled = !connected && !running && state.pendingHostKey == null, running = running, connected = connected)
                if (running) LinearProgressIndicator(Modifier.fillMaxWidth())
                if (connected) {
                    Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(stringResource(R.string.connection_active), style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(R.string.herdr_connected_version, state.herdrVersion.orEmpty()))
                            Button(onClick = viewModel::openDesktop, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.open_desktop)) }
                        }
                    }
                    AgentsCard(viewModel, state)
                }
                state.failure?.let { failure -> FailureCard(failure, if (failure.kind == FailureKind.HOST_KEY_CHANGED) ({ state.selectedHost?.let(viewModel::forget) }) else null) }
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
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.device_key_title), style = MaterialTheme.typography.titleMedium)
            Text(key.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
            SelectionContainer { Text(key.publicKeyOpenssh, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
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
private fun AgentsCard(vm: HostCheckViewModel, state: CoreSnapshot) {
    var prompt by remember { mutableStateOf("") }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.agents_title), style = MaterialTheme.typography.titleMedium)
            if (state.agents.isEmpty()) Text(stringResource(R.string.no_agents))
            state.agents.forEach { agent ->
                OutlinedButton(onClick = { vm.selectAgent(agent.id) }, modifier = Modifier.fillMaxWidth()) {
                    Column { Text(agent.name ?: agent.title ?: agent.agent ?: agent.id); Text(agent.status, style = MaterialTheme.typography.bodySmall) }
                }
            }
            state.selectedAgent?.let { id ->
                Text(stringResource(R.string.terminal_output), style = MaterialTheme.typography.labelLarge)
                SelectionContainer { Text(state.agentOutput, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.fillMaxWidth().heightIn(max = 240.dp).verticalScroll(rememberScrollState())) }
                OutlinedTextField(prompt, { prompt = it }, label = { Text(stringResource(R.string.agent_prompt)) }, modifier = Modifier.fillMaxWidth())
                Button(onClick = { val text = prompt; prompt = ""; vm.promptAgent(id, text) }, enabled = prompt.isNotBlank()) { Text(stringResource(R.string.send_agent_prompt)) }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DesktopScreen(vm: HostCheckViewModel, state: CoreSnapshot) {
    var textEntry by remember { mutableStateOf(false) }
    var text by remember { mutableStateOf("") }
    var sendError by remember { mutableStateOf(false) }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.desktop_title)) }, navigationIcon = {
        TextButton(onClick = vm::closeDesktop) { Text(stringResource(R.string.back)) }
    }, actions = {
        TextButton(onClick = { vm.takeOver(!state.desktop.takeover) }, enabled = state.desktop.status == DesktopStatus.STREAMING) {
            Text(stringResource(if (state.desktop.takeover) R.string.stop_takeover else R.string.takeover))
        }
    }) }) { padding ->
        Column(Modifier.padding(padding).imePadding().fillMaxSize()) {
            Text(stringResource(R.string.video_frame_counts, state.desktop.receivedFrames.toLong(), state.desktop.renderedFrames.toLong()), style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 12.dp))
            if (state.desktop.sourceWidth > 0u) Text(
                stringResource(R.string.video_dimensions, state.desktop.sourceWidth.toInt(), state.desktop.sourceHeight.toInt(), state.desktop.width.toInt(), state.desktop.height.toInt(), state.desktop.fps.toInt()),
                style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 12.dp))
            BoxWithConstraints(Modifier.weight(1f).fillMaxWidth().background(Color.Black)) {
                val ratio = state.desktop.sourceWidth.toFloat().coerceAtLeast(1f) / state.desktop.sourceHeight.toFloat().coerceAtLeast(1f)
                val wide = maxWidth.value / maxHeight.value.coerceAtLeast(1f) > ratio
                val videoModifier = if (wide) Modifier.fillMaxHeight().aspectRatio(ratio) else Modifier.fillMaxWidth().aspectRatio(ratio)
                AndroidView(factory = { VideoSurface(it, vm::videoReady, vm::videoGone) }, update = { it.takeover = state.desktop.takeover },
                    modifier = videoModifier.align(Alignment.Center))
                when (state.desktop.status) {
                    DesktopStatus.PAIRING -> Card(Modifier.align(Alignment.Center).padding(24.dp)) {
                        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(stringResource(R.string.pairing_title), style = MaterialTheme.typography.titleLarge)
                            Text(state.desktop.pairingName.orEmpty(), style = MaterialTheme.typography.bodySmall)
                            Text(state.desktop.pin.orEmpty(), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.displayMedium)
                            Text(stringResource(R.string.pairing_hint))
                            Button(onClick = { state.desktop.pin?.let(vm::approve) }) { Text(stringResource(R.string.auto_confirm_pairing)) }
                        }
                    }
                    DesktopStatus.STARTING, DesktopStatus.IDLE, DesktopStatus.STOPPING -> Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                        CircularProgressIndicator(); Text(stringResource(R.string.desktop_starting), color = Color.White)
                    }
                    DesktopStatus.FAILED -> Card(Modifier.align(Alignment.Center).padding(24.dp)) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(stringResource(R.string.error_desktop))
                            state.failure?.let { FailureCard(it, null) }
                            Button(onClick = vm::retryDesktop) { Text(stringResource(R.string.retry)) }
                        }
                    }
                    DesktopStatus.STREAMING -> Unit
                }
            }
            Text(stringResource(if (state.desktop.takeover) R.string.takeover_hint else R.string.watch_hint), style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(8.dp))
            if (state.desktop.takeover) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    TextButton(onClick = vm::rightClick) { Text(stringResource(R.string.right_click)) }
                    TextButton(onClick = { vm.scroll(120) }) { Text(stringResource(R.string.scroll_up)) }
                    TextButton(onClick = { vm.scroll(-120) }) { Text(stringResource(R.string.scroll_down)) }
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    TextButton(onClick = { vm.shortcut(27) }) { Text(stringResource(R.string.shortcut_esc)) }
                    TextButton(onClick = { vm.shortcut(9) }) { Text(stringResource(R.string.shortcut_tab)) }
                    TextButton(onClick = { vm.shortcut(13) }) { Text(stringResource(R.string.shortcut_enter)) }
                    TextButton(onClick = { vm.shortcut(67, true) }) { Text(stringResource(R.string.shortcut_ctrl_c)) }
                    TextButton(onClick = { textEntry = !textEntry }) { Text(stringResource(R.string.keyboard)) }
                }
                if (textEntry) Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(text, { text = it; sendError = false }, label = { Text(stringResource(R.string.remote_text)) }, modifier = Modifier.weight(1f), isError = sendError)
                    TextButton(onClick = { if (text.toByteArray(Charsets.UTF_8).size <= 4096 && vm.sendText(text)) text = "" else sendError = true }, enabled = text.isNotEmpty()) { Text(stringResource(R.string.send_text)) }
                }
            }
        }
    }
    val platformError by vm.engine.platformError.collectAsState()
    platformError?.let { AlertDialog(onDismissRequest = { vm.clearLocalError() }, title = { Text(stringResource(R.string.error_action)) }, text = { Text(it) }, confirmButton = { TextButton(onClick = { vm.clearLocalError() }) { Text(stringResource(R.string.dismiss)) } }) }
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
        FailureKind.CONNECTION -> stringResource(R.string.error_connection_generic)
        FailureKind.INVALID_CONFIGURATION -> stringResource(R.string.error_action)
    }
    var details by remember { mutableStateOf(false) }
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
