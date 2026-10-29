package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*

@Composable
fun ComposerSettings(vm: HostCheckViewModel, agent: AgentInfo?, composer: ComposerInfo?) {
    if (agent?.agent != "codex" || composer == null) return
    if (!composer.ready) {
        var details by remember { mutableStateOf(false) }
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            if (composer.loading) CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 1.5.dp)
            Text(stringResource(if (composer.loading) R.string.composer_loading else R.string.composer_backend_unavailable), fontSize = 12.sp, color = Color(0xFF888982), modifier = Modifier.weight(1f).padding(start = if (composer.loading) 8.dp else 0.dp))
            if (!composer.loading) TextButton(onClick = vm::reloadComposer) { Text(stringResource(R.string.retry)) }
            if (composer.details.isNotBlank()) IconButton(onClick = { details = true }, modifier = Modifier.size(36.dp)) { ProductIcon(Mark.More, modifier = Modifier.size(17.dp)) }
        }
        if (details) AlertDialog(onDismissRequest = { details = false }, title = { Text(stringResource(R.string.connection_diagnostics)) }, text = { SelectionContainer { Text(composer.details, fontSize = 12.sp, modifier = Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState())) } }, confirmButton = { TextButton(onClick = { details = false; vm.reloadComposer() }) { Text(stringResource(R.string.retry)) } }, dismissButton = { TextButton(onClick = { details = false }) { Text(stringResource(R.string.dismiss)) } })
        return
    }
    val model = composer.models.find { it.model == composer.model }
    var models by remember(agent.id) { mutableStateOf(false) }
    var efforts by remember(agent.id) { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f, fill = false)) {
            TextButton(onClick = { models = true }, enabled = !vm.sending, contentPadding = PaddingValues(horizontal = 8.dp, vertical = 0.dp)) {
                Text(model?.name ?: composer.model, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                ProductIcon(Mark.Chevron, modifier = Modifier.padding(start = 4.dp).size(13.dp))
            }
            DropdownMenu(models, { models = false }) {
                composer.models.forEach { option ->
                    DropdownMenuItem(text = { Text(option.name, fontSize = 14.sp) }, trailingIcon = { if (option.model == composer.model) ProductIcon(Mark.Check, modifier = Modifier.size(17.dp)) }, onClick = {
                        models = false
                        vm.selectModel(agent.id, option.model, composer.effort.takeIf { it in option.efforts } ?: option.defaultEffort)
                    })
                }
            }
        }
        Text("·", color = Color(0xFFC5C5BE), modifier = Modifier.padding(horizontal = 2.dp))
        Box {
            TextButton(onClick = { efforts = true }, enabled = !vm.sending, contentPadding = PaddingValues(horizontal = 8.dp, vertical = 0.dp)) {
                Text(effortLabel(composer.effort), fontSize = 12.sp)
                ProductIcon(Mark.Chevron, modifier = Modifier.padding(start = 4.dp).size(13.dp))
            }
            DropdownMenu(efforts, { efforts = false }) {
                model?.efforts?.forEach { effort ->
                    DropdownMenuItem(text = { Text(effortLabel(effort)) }, trailingIcon = { if (effort == composer.effort) ProductIcon(Mark.Check, modifier = Modifier.size(17.dp)) }, onClick = { efforts = false; vm.selectModel(agent.id, composer.model, effort) })
                }
            }
        }
    }
}

@Composable private fun effortLabel(effort: String): String = when (effort) {
    "none" -> stringResource(R.string.effort_none)
    "minimal" -> stringResource(R.string.effort_minimal)
    "low" -> stringResource(R.string.effort_low)
    "medium" -> stringResource(R.string.effort_medium)
    "high" -> stringResource(R.string.effort_high)
    "xhigh" -> stringResource(R.string.effort_xhigh)
    "max" -> stringResource(R.string.effort_max)
    "ultra" -> stringResource(R.string.effort_ultra)
    else -> effort
}

@Composable fun AttachmentDrafts(vm: HostCheckViewModel, files: List<UploadInfo>) {
    if (files.isEmpty() && !vm.copyingAttachments) return
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            files.forEach { upload ->
                Surface(color = Color(0xFFF5F4EF), shape = RoundedCornerShape(12.dp), modifier = Modifier.widthIn(max = 235.dp)) {
                    Row(Modifier.padding(start = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                        ProductIcon(attachmentMark(upload.file.mime), Color(0xFF777870), Modifier.size(20.dp))
                        Column(Modifier.widthIn(max = 154.dp).padding(start = 9.dp, top = 9.dp, bottom = 9.dp)) {
                            Text(upload.file.name, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (upload.failed) TextButton(onClick = { vm.retryAttachment(upload) }, enabled = !vm.sending, contentPadding = PaddingValues(0.dp), modifier = Modifier.height(24.dp)) { Text(stringResource(R.string.attachment_retry), fontSize = 11.sp, color = MaterialTheme.colorScheme.error) }
                            else Text(stringResource(if (upload.ready) R.string.attachment_ready else R.string.attachment_uploading, if (upload.file.size > 0u) (upload.sent * 100u / upload.file.size).toInt() else 0), fontSize = 11.sp, color = Color(0xFF888982))
                        }
                        IconButton(onClick = { vm.removeAttachment(upload) }, enabled = !vm.sending && (upload.ready || upload.failed), modifier = Modifier.size(40.dp)) { ProductIcon(Mark.Close, modifier = Modifier.size(14.dp)) }
                    }
                }
            }
        }
        if (vm.copyingAttachments) LinearProgressIndicator(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp))
        if (files.any { it.file.mime.startsWith("audio/") }) Text(stringResource(R.string.audio_attachment_hint), color = Color(0xFF888982), fontSize = 11.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
    }
}

fun attachmentMark(mime: String) = when { mime.startsWith("image/") -> Mark.Image; mime.startsWith("audio/") -> Mark.Audio; else -> Mark.File }

@Composable fun MessageAttachments(files: List<AttachmentInfo>) {
    files.forEach { file ->
        Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            ProductIcon(attachmentMark(file.mime), Color(0xFF6484A3), Modifier.size(18.dp))
            Text(file.name, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 8.dp))
        }
    }
}
