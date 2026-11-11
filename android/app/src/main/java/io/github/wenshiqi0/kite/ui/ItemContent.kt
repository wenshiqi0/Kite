package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.res.stringResource
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.ui.theme.KiteColors

/** Render every supported block in its original position, including mixed items. */
@Composable
internal fun ItemContent(
    message: ItemInfo,
    streaming: Boolean = false,
    inspect: () -> Unit = {},
    desktop: () -> Unit = {},
    desktopEnabled: Boolean = true,
    attachment: @Composable (List<AttachmentInfo>) -> Unit,
) {
    Column {
        itemContentRuns(message.blocks).forEach { run ->
            val index = run.start
            val block = run.blocks.first()
            key(index) {
                when (block) {
                    is BlockInfo.Text -> if (block.text.isNotBlank()) {
                        MarkdownMessage(block.text, streaming = streaming)
                    }
                    is BlockInfo.Attachment -> attachment(run.blocks.filterIsInstance<BlockInfo.Attachment>().map { it.file })
                    is BlockInfo.SkillContext -> SkillContextCard(block)
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
