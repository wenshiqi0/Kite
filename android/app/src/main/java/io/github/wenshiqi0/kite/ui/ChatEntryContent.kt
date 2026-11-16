package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Main chat and read-only task history share the entire row, including notices. */
@Composable
internal fun ChatEntryContent(
    entry: ChatEntry,
    working: Boolean = false,
    streaming: Boolean = false,
    inspect: () -> Unit = {},
    desktop: () -> Unit = {},
    desktopEnabled: Boolean = true,
    attachment: @Composable (List<AttachmentInfo>) -> Unit,
) {
    when (entry) {
        is ChatEntry.Work -> ToolGroupCard(entry, working, inspect, desktop, desktopEnabled)
        is ChatEntry.Message -> ChatMessageCard(entry.message, streaming, inspect, desktop, desktopEnabled, attachment)
    }
}

@Composable
private fun ChatMessageCard(
    message: ItemInfo,
    streaming: Boolean,
    inspect: () -> Unit,
    desktop: () -> Unit,
    desktopEnabled: Boolean,
    attachment: @Composable (List<AttachmentInfo>) -> Unit,
) {
    if (message.isSkillContextOnly) {
        ItemContent(message, attachment = attachment)
        return
    }
    if (remember(message) { isGlobalInstructionsMessage(message) }) {
        GlobalInstructionsNotice()
        return
    }
    if (message.role == ItemRole.ERROR) {
        TurnFailureMessage(message.text)
        return
    }
    // A tool result with media keeps the same work-card width as its neighbors.
    if (message.role == ItemRole.TOOL) {
        Column(Modifier.fillMaxWidth()) {
            ItemContent(message, streaming, inspect, desktop, desktopEnabled, attachment)
        }
        return
    }
    val user = message.role == ItemRole.USER
    Row(Modifier.fillMaxWidth(), horizontalArrangement = if (user) Arrangement.End else Arrangement.Start) {
        Surface(
            color = if (user) KiteColors.UserBubble else KiteColors.Surface,
            shape = RoundedCornerShape(19.dp),
            modifier = Modifier.widthIn(max = 340.dp),
        ) {
            Column(Modifier.padding(horizontal = if (user) 12.dp else 15.dp, vertical = if (user) 10.dp else 11.dp)) {
                ItemContent(message, streaming, inspect, desktop, desktopEnabled, attachment)
            }
        }
    }
}
