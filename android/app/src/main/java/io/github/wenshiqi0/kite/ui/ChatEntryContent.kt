package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
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
    attachment: @Composable (AttachmentInfo) -> Unit,
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
    attachment: @Composable (AttachmentInfo) -> Unit,
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
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(5.dp)) {
        Text(stringResource(if (message.role == ItemRole.USER) R.string.chat_author_you else R.string.chat_author_agent),
            color = KiteColors.Muted, fontSize = 12.sp)
        Surface(
            color = KiteColors.Surface,
            shape = RoundedCornerShape(19.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(Modifier.padding(horizontal = 15.dp, vertical = 11.dp)) {
                ItemContent(message, streaming, inspect, desktop, desktopEnabled, attachment)
            }
        }
    }
}
