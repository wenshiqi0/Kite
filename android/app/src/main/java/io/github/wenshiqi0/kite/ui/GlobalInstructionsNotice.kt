package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Display-only recognition of Codex's global guidance envelope, independent of its language. */
fun isGlobalInstructionsMessage(message: ItemInfo): Boolean {
    if (message.role != ItemRole.USER || message.blocks.any { it !is io.github.wenshiqi0.kite.core.BlockInfo.Text }) return false
    val text = message.text.trim()
    if (text.substringBefore('\n').trimEnd() != "# AGENTS.md instructions") return false
    val body = text.substringAfter('\n', "").trimStart()
    if (!body.startsWith("<INSTRUCTIONS>")) return false
    val end = body.indexOf("</INSTRUCTIONS>")
    if (end < 0) return false
    val suffix = body.substring(end + "</INSTRUCTIONS>".length).trim()
    if (suffix.isEmpty()) return true
    return suffix.startsWith("<environment_context>") && suffix.endsWith("</environment_context>") &&
        suffix.indexOf("</environment_context>") == suffix.length - "</environment_context>".length
}

@Composable
fun GlobalInstructionsNotice() {
    Row(Modifier.fillMaxWidth().padding(horizontal = 15.dp, vertical = 4.dp), horizontalArrangement = Arrangement.Center) {
        Text(stringResource(R.string.global_instructions_loaded), color = KiteColors.Muted, fontSize = 12.sp, lineHeight = 18.sp)
    }
}
