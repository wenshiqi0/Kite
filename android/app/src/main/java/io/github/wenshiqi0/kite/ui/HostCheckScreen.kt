package io.github.wenshiqi0.kite.ui

import android.content.ClipData
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.HerdrCheck
import io.github.wenshiqi0.kite.core.SshKeyPair
import io.github.wenshiqi0.kite.data.HostForm
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostCheckScreen(viewModel: HostCheckViewModel = viewModel()) {
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.app_name)) }) }) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            DeviceKeyCard(viewModel.deviceKey, viewModel.deviceKeyError)
            HostFormCard(
                form = viewModel.form,
                onFormChange = viewModel::updateForm,
                running = viewModel.state == CheckState.Running,
                canCheck = viewModel.deviceKey != null,
                onCheck = viewModel::check,
            )
            when (val state = viewModel.state) {
                CheckState.Idle, is CheckState.ConfirmHostKey -> Unit
                CheckState.Running -> LinearProgressIndicator(Modifier.fillMaxWidth())
                is CheckState.Passed -> PassedCard(state.check)
                is CheckState.Failed -> FailedCard(state, onForgetHostKey = viewModel::forgetHostKey)
            }
        }
    }

    (viewModel.state as? CheckState.ConfirmHostKey)?.let { confirm ->
        HostKeyDialog(
            confirm = confirm,
            onTrust = { viewModel.trustHostKey(confirm) },
            onDismiss = viewModel::dismissHostKey,
        )
    }
}

@Composable
private fun DeviceKeyCard(key: SshKeyPair?, error: String?) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.device_key_title), style = MaterialTheme.typography.titleMedium)
            when {
                error != null -> Text(stringResource(R.string.device_key_error, error), color = MaterialTheme.colorScheme.error)
                key == null -> Text(stringResource(R.string.device_key_generating))
                else -> {
                    Text(key.fingerprint, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                    SelectionContainer {
                        Text(key.publicKeyOpenssh, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                    }
                    Text(
                        stringResource(R.string.device_key_hint),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    OutlinedButton(onClick = {
                        scope.launch {
                            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("ssh public key", key.publicKeyOpenssh)))
                        }
                    }) {
                        Text(stringResource(R.string.device_key_copy))
                    }
                }
            }
        }
    }
}

@Composable
private fun HostFormCard(
    form: HostForm,
    onFormChange: (HostForm) -> Unit,
    running: Boolean,
    canCheck: Boolean,
    onCheck: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.host_title), style = MaterialTheme.typography.titleMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = form.host,
                    onValueChange = { onFormChange(form.copy(host = it)) },
                    label = { Text(stringResource(R.string.host_address)) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.weight(3f),
                )
                OutlinedTextField(
                    value = form.port,
                    onValueChange = { onFormChange(form.copy(port = it)) },
                    label = { Text(stringResource(R.string.host_port)) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    modifier = Modifier.weight(1f),
                )
            }
            OutlinedTextField(
                value = form.username,
                onValueChange = { onFormChange(form.copy(username = it)) },
                label = { Text(stringResource(R.string.host_username)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = form.herdrPath,
                onValueChange = { onFormChange(form.copy(herdrPath = it)) },
                label = { Text(stringResource(R.string.host_herdr_path)) },
                placeholder = { Text(stringResource(R.string.host_herdr_path_placeholder)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Button(onClick = onCheck, enabled = canCheck && !running, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(if (running) R.string.check_running else R.string.check_herdr))
            }
        }
    }
}

@Composable
private fun PassedCard(check: HerdrCheck) {
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(R.string.check_passed), style = MaterialTheme.typography.titleMedium)
            Detail(R.string.check_capability, check.capability)
            Detail(R.string.check_path, check.herdrPath)
            check.version?.let { Detail(R.string.check_version, it) }
            Detail(R.string.check_host_key, check.hostKeyFingerprint)
        }
    }
}

@Composable
private fun Detail(label: Int, value: String) {
    Column {
        Text(stringResource(label), style = MaterialTheme.typography.labelMedium)
        Text(value, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
    }
}

@Composable
private fun FailedCard(state: CheckState.Failed, onForgetHostKey: () -> Unit) {
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(state.message, color = MaterialTheme.colorScheme.onErrorContainer)
            if (state.hostKeyChanged) {
                TextButton(onClick = onForgetHostKey) {
                    Text(stringResource(R.string.forget_host_key))
                }
            }
        }
    }
}

@Composable
private fun HostKeyDialog(confirm: CheckState.ConfirmHostKey, onTrust: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.host_key_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.host_key_body, confirm.address))
                Text(confirm.fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.host_key_hint), style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = { TextButton(onClick = onTrust) { Text(stringResource(R.string.host_key_trust)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}
