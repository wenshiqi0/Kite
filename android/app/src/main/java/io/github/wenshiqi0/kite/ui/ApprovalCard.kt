package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ApprovalInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

@Composable
fun ApprovalCard(approval: ApprovalInfo, enabled: Boolean, choose: (Int) -> Unit) {
    Surface(color = KiteColors.WarningSurface, shape = RoundedCornerShape(19.dp), border = BorderStroke(1.dp, KiteColors.Warning), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.approval_title), fontWeight = FontWeight.SemiBold, color = KiteColors.Warning)
            key(approval.id) {
                Column(Modifier.heightIn(max = 300.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    approval.operation?.let { Text(operationLabel(it), fontSize = 17.sp, fontWeight = FontWeight.SemiBold, color = KiteColors.Text) }
                    approval.explanation?.takeIf { it.isNotBlank() }?.let { MarkdownMessage(it, color = KiteColors.Text, size = 14f) }
                    approval.command?.let { command ->
                        SelectionContainer {
                            Text(command, fontFamily = FontFamily.Monospace, fontSize = 13.sp, color = KiteColors.Text,
                                modifier = Modifier.fillMaxWidth().background(KiteColors.Surface, RoundedCornerShape(10.dp)).padding(12.dp))
                        }
                    }
                    approval.details.forEach { detail ->
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(detailLabel(detail.name), fontSize = 12.sp, color = KiteColors.Muted)
                            SelectionContainer { Text(detail.value, fontSize = 13.sp, color = KiteColors.Text, fontFamily = FontFamily.Monospace) }
                        }
                    }
                    approval.workingDirectory?.takeIf { it.isNotBlank() }?.let {
                        Text(stringResource(R.string.approval_directory, it), fontSize = 12.sp, color = KiteColors.Muted)
                    }
                    if (approval.operation == null && approval.command == null) SelectionContainer { Text(approval.prompt, fontSize = 13.sp, color = KiteColors.Text) }
                }
            }
            if (approval.operation != null) Text(stringResource(R.string.approval_once_hint), fontSize = 12.sp, color = KiteColors.Muted)
            if (approval.choices.size == 2) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                approval.choices.forEachIndexed { index, choice ->
                    ApprovalButton(choice.label, index, approval.operation != null, enabled, Modifier.weight(1f)) { choose(index) }
                }
            } else approval.choices.forEachIndexed { index, choice ->
                ApprovalButton(choice.label, index, approval.operation != null, enabled, Modifier.fillMaxWidth()) { choose(index) }
            }
        }
    }
}

@Composable private fun ApprovalButton(label: String, index: Int, once: Boolean, enabled: Boolean, modifier: Modifier, choose: () -> Unit) {
    Button(onClick = choose, enabled = enabled, modifier = modifier, shape = RoundedCornerShape(11.dp),
        colors = ButtonDefaults.buttonColors(containerColor = if (index == 0) KiteColors.Accent else KiteColors.Raised,
            contentColor = if (index == 0) KiteColors.OnAccent else KiteColors.Text)) {
        Text(when (label.trim().lowercase()) {
            "yes" -> stringResource(if (once) R.string.approval_allow_once else R.string.approval_allow)
            "no" -> stringResource(R.string.approval_deny)
            else -> label
        })
    }
}

@Composable private fun operationLabel(operation: String): String = stringResource(when (operation.lowercase()) {
    "bash", "shell", "terminal" -> R.string.approval_run_command
    "read" -> R.string.approval_read_file
    "create", "write" -> R.string.approval_write_file
    "edit" -> R.string.approval_edit_file
    "remove", "delete" -> R.string.approval_delete_file
    "agent" -> R.string.approval_delegate
    else -> R.string.approval_run_tool
})

@Composable private fun detailLabel(name: String): String = when (name) {
    "path", "file_path" -> stringResource(R.string.approval_file)
    "content" -> stringResource(R.string.approval_content)
    "old_string", "oldText" -> stringResource(R.string.approval_old_content)
    "new_string", "newText" -> stringResource(R.string.approval_new_content)
    "prompt" -> stringResource(R.string.approval_task)
    else -> name
}
