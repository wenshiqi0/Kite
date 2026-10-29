package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R

@Composable
fun ToolGroupCard(group: ChatEntry.Work, working: Boolean, inspect: () -> Unit, desktop: () -> Unit) {
    var expanded by rememberSaveable(group.key) { mutableStateOf(false) }
    var selectedCall by rememberSaveable(group.key) { mutableStateOf<String?>(null) }
    val calls = group.calls
    val running = calls.lastOrNull { it.tool?.complete == false }
    val failed = calls.count { it.tool?.failed == true }
    val colour = if (failed > 0 && running == null) MaterialTheme.colorScheme.error else Color(0xFF888982)
    val toggle = stringResource(if (expanded) R.string.collapse_tool_group else R.string.expand_tool_group, calls.size)
    Surface(color = Color(0xFFFAFAF7), shape = RoundedCornerShape(14.dp), border = BorderStroke(1.dp, Color(0xFFE8E8E1)), modifier = Modifier.fillMaxWidth()) {
        Column {
            Row(Modifier.fillMaxWidth().clickable(role = Role.Button, onClickLabel = toggle) { if (!expanded) inspect(); expanded = !expanded }.semantics { contentDescription = toggle }.padding(start = 14.dp, end = 8.dp, top = 10.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                if (running != null && working) CircularProgressIndicator(Modifier.size(17.dp), strokeWidth = 1.5.dp)
                else ProductIcon(if (running != null) Mark.Chat else if (failed > 0) Mark.Terminal else Mark.Check, colour, Modifier.size(17.dp))
                Column(Modifier.weight(1f).padding(start = 10.dp)) {
                    Text(stringResource(R.string.tool_group_count, calls.size), fontSize = 13.sp, color = Color(0xFF545750))
                    val status = if (running != null && working) stringResource(R.string.tool_group_running) else if (failed > 0) stringResource(R.string.tool_group_failed, failed) else if (running != null) stringResource(R.string.tool_group_pending) else stringResource(R.string.tool_group_completed)
                    Text(status + calls.lastOrNull()?.tool?.summary?.takeIf { it.isNotBlank() }?.let { " · $it" }.orEmpty(), fontSize = 11.sp, color = colour, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (calls.any { it.tool?.computer == true }) {
                    val label = stringResource(R.string.open_desktop)
                    IconButton(onClick = desktop, modifier = Modifier.size(40.dp).semantics { contentDescription = label }) { ProductIcon(Mark.Monitor, modifier = Modifier.size(18.dp)) }
                }
                ProductIcon(Mark.Chevron, colour, Modifier.padding(horizontal = 10.dp).size(16.dp).rotate(if (expanded) 180f else 0f))
            }
            if (expanded) {
                HorizontalDivider(color = Color(0xFFE8E8E1))
                // Even a turn with hundreds of calls occupies one bounded timeline row.
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 320.dp), contentPadding = PaddingValues(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    items(group.messages, key = { it.id }) { message ->
                        val tool = message.tool
                        if (tool == null) Text(message.text, color = Color(0xFF888982), fontSize = 12.sp, modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp))
                        else Row(Modifier.fillMaxWidth().clickable { inspect(); selectedCall = message.id }.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            ProductIcon(if (tool.computer) Mark.Monitor else Mark.Terminal, if (tool.failed) MaterialTheme.colorScheme.error else Color(0xFF888982), Modifier.size(16.dp))
                            Text(tool.label + tool.summary.takeIf { it.isNotBlank() }?.let { " · $it" }.orEmpty(), fontSize = 12.sp, color = Color(0xFF545750), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 8.dp))
                            if (!tool.complete && working) CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.dp)
                            else if (tool.complete && !tool.failed) ProductIcon(Mark.Check, Color(0xFF888982), Modifier.size(13.dp))
                        }
                    }
                }
            }
        }
    }
    group.messages.find { it.id == selectedCall }?.tool?.let { tool ->
        AlertDialog(onDismissRequest = { selectedCall = null }, title = { Text(tool.label) }, text = {
            SelectionContainer { Column(Modifier.heightIn(max = 440.dp).verticalScroll(rememberScrollState())) {
                Text(stringResource(R.string.tool_arguments), color = Color(0xFF888982), fontSize = 12.sp)
                Text(tool.arguments, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                if (tool.output.isNotBlank()) { Spacer(Modifier.height(16.dp)); Text(stringResource(R.string.tool_output), color = Color(0xFF888982), fontSize = 12.sp); Text(tool.output, fontFamily = FontFamily.Monospace, fontSize = 12.sp) }
            } }
        }, confirmButton = { TextButton(onClick = { selectedCall = null }) { Text(stringResource(R.string.dismiss)) } })
    }
}
