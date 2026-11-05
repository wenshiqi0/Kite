package io.github.wenshiqi0.kite.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Render every supported block in its original position, including mixed items. */
@Composable
internal fun ItemContent(
    message: ItemInfo,
    streaming: Boolean = false,
    inspect: () -> Unit = {},
    desktop: () -> Unit = {},
    desktopEnabled: Boolean = true,
    attachment: @Composable (AttachmentInfo) -> Unit,
) {
    val context = LocalContext.current
    val copyLabel = stringResource(R.string.copy_reply)
    Column {
        message.blocks.forEachIndexed { index, block ->
            key(index) {
                when (block) {
                    is BlockInfo.Text -> if (block.text.isNotBlank()) {
                        if (message.role == ItemRole.USER) SelectionContainer {
                            Text(block.text, color = KiteColors.Accent, fontSize = 16.sp, lineHeight = 25.sp)
                        } else {
                            MarkdownMessage(block.text, streaming = streaming)
                            IconButton(onClick = {
                                (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager)
                                    .setPrimaryClip(ClipData.newPlainText(copyLabel, block.text))
                            }, modifier = Modifier.align(Alignment.End).size(40.dp).semantics { contentDescription = copyLabel }) {
                                ProductIcon(Mark.Clipboard, KiteColors.Muted, Modifier.size(17.dp))
                            }
                        }
                    }
                    is BlockInfo.Attachment -> attachment(block.file)
                    is BlockInfo.QuestionReply -> AnsweredQuestions(listOf(block.reply))
                    is BlockInfo.Tool -> ToolGroupCard(
                        ChatEntry.Work(listOf(message.copy(id = "${message.id}:block:$index", blocks = listOf(block)))),
                        streaming, inspect, desktop, desktopEnabled,
                    )
                    // Reserved until the snapshot protocol lands in phase 3.
                    is BlockInfo.Embed -> Text(stringResource(R.string.chat_reference_pending), color = KiteColors.Muted)
                }
            }
        }
    }
}
