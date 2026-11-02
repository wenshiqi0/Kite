package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.QuestionReplyInfo
import io.github.wenshiqi0.kite.core.ToolInfo

/** Presentation reads of one item's typed blocks; Rust owns what the blocks are. */
val ItemInfo.text:String get()=blocks.filterIsInstance<BlockInfo.Text>().joinToString("\n"){it.text}
/** Only a pure tool item may be collapsed; mixed content stays visible in order. */
val ItemInfo.tool:ToolInfo? get()=(blocks.singleOrNull() as? BlockInfo.Tool)?.tool
val ItemInfo.attachments:List<AttachmentInfo> get()=blocks.filterIsInstance<BlockInfo.Attachment>().map{it.file}
val ItemInfo.questionReplies:List<QuestionReplyInfo> get()=blocks.filterIsInstance<BlockInfo.QuestionReply>().map{it.reply}
