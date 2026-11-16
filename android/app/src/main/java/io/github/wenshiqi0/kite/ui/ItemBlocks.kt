package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.QuestionReplyInfo
import io.github.wenshiqi0.kite.core.ToolInfo

/** Presentation reads of one item's typed blocks; Rust owns what the blocks are. */
val ItemInfo.text:String get()=blocks.filterIsInstance<BlockInfo.Text>().joinToString("\n"){it.text}
val ItemInfo.isSkillContextOnly: Boolean
    get() = blocks.isNotEmpty() && blocks.all { it is BlockInfo.SkillContext }
/** Only a pure tool item may be collapsed; mixed content stays visible in order. */
val ItemInfo.tool:ToolInfo? get()=(blocks.singleOrNull() as? BlockInfo.Tool)?.tool
val ItemInfo.attachments:List<AttachmentInfo> get()=blocks.filterIsInstance<BlockInfo.Attachment>().map{it.file}
val ItemInfo.questionReplies:List<QuestionReplyInfo> get()=blocks.filterIsInstance<BlockInfo.QuestionReply>().map{it.reply}

internal data class ItemContentRun(val start: Int, val blocks: List<BlockInfo>)

/** Adjacent attachments form one album; meaningful content keeps its position. */
internal fun itemContentRuns(blocks: List<BlockInfo>): List<ItemContentRun> = buildList {
    var index = 0
    while (index < blocks.size) {
        val start = index
        if (blocks[index] !is BlockInfo.Attachment) {
            add(ItemContentRun(index, listOf(blocks[index])))
            index++
            continue
        }
        val files = mutableListOf(blocks[index++])
        while (index < blocks.size) {
            when {
                blocks[index] is BlockInfo.Attachment -> files.add(blocks[index++])
                blocks[index] is BlockInfo.Text && (blocks[index] as BlockInfo.Text).text.isBlank() -> {
                    var next = index
                    while (blocks.getOrNull(next) is BlockInfo.Text && (blocks[next] as BlockInfo.Text).text.isBlank()) next++
                    if (blocks.getOrNull(next) is BlockInfo.Attachment) index = next else break
                }
                else -> break
            }
        }
        add(ItemContentRun(start, files))
    }
}
