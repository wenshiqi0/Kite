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

internal fun isInlineFile(file: AttachmentInfo): Boolean = when (attachmentPreviewKind(file.mime, file.name)) {
    AttachmentPreviewKind.Text, AttachmentPreviewKind.Other -> true
    else -> false
}

private fun BlockInfo.isInlineContent(): Boolean = this is BlockInfo.Text || this is BlockInfo.Attachment && isInlineFile(file)

/** Keep prose and document references in one Markdown layout; media keeps its position. */
internal fun itemContentRuns(blocks: List<BlockInfo>): List<ItemContentRun> = buildList {
    var index = 0
    while (index < blocks.size) {
        val start = index
        if (blocks[index].isInlineContent()) {
            while (index < blocks.size && blocks[index].isInlineContent()) index++
            add(ItemContentRun(start, blocks.subList(start, index)))
            continue
        }
        if (blocks[index] !is BlockInfo.Attachment) {
            add(ItemContentRun(index, listOf(blocks[index])))
            index++
            continue
        }
        val files = mutableListOf(blocks[index++])
        while (index < blocks.size) {
            when {
                blocks[index] is BlockInfo.Attachment && !blocks[index].isInlineContent() -> files.add(blocks[index++])
                blocks[index] is BlockInfo.Text && (blocks[index] as BlockInfo.Text).text.isBlank() -> {
                    var next = index
                    while (blocks.getOrNull(next) is BlockInfo.Text && (blocks[next] as BlockInfo.Text).text.isBlank()) next++
                    if (blocks.getOrNull(next) is BlockInfo.Attachment && !blocks[next].isInlineContent()) index = next else break
                }
                else -> break
            }
        }
        add(ItemContentRun(start, files))
    }
}

/** The destinations are scoped to this rendered run, never paths or external URLs. */
internal fun inlineFileMarkdown(blocks: List<BlockInfo>): String = buildString {
    var fileIndex = 0
    var previousFile = false
    for (block in blocks) {
        when (block) {
            is BlockInfo.Text -> {
                append(block.text)
                previousFile = false
            }
            is BlockInfo.Attachment -> {
                if (previousFile) append(' ')
                val label = block.file.name.replace('\n', ' ').replace('\r', ' ')
                    .replace(Regex("([\\p{Punct}])"), "\\\\$1")
                append('[').append(label).append("](kite-attachment:").append(fileIndex++).append(')')
                previousFile = true
            }
            else -> error("Only text and inline files belong in a Markdown run")
        }
    }
}
