package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.*
import org.junit.Assert.*
import org.junit.Test

class ChatTimelineTest {
    private fun text(id: String, user: Boolean = false, intermediate: Boolean = false) = ItemInfo(id = id, role = if (user) ItemRole.USER else ItemRole.ASSISTANT, blocks = listOf(BlockInfo.Text(id)), intermediate = intermediate, origin = null)
    private fun tool(id: String, complete: Boolean = true) = ItemInfo(id = id, role = ItemRole.TOOL, blocks = listOf(BlockInfo.Tool(ToolInfo(label = "Bash", summary = id, name = "exec_command", arguments = id, output = if (complete) "ok" else "", complete = complete, failed = false, computer = false))), intermediate = false, origin = null)

    @Test fun progressTextRemainsVisibleBetweenCollapsedToolGroups() {
        val calls = (1..80).flatMap { listOf(text("progress-$it", intermediate = true), tool("tool-$it")) }
        val entries = chatEntries(listOf(text("user", user = true)) + calls + text("answer"))
        assertEquals(162, entries.size)
        assertEquals(80, entries.filterIsInstance<ChatEntry.Work>().size)
        assertEquals(82, entries.filterIsInstance<ChatEntry.Message>().size)
        assertEquals(calls.filter { it.tool == null }.map { it.id }, entries.filterIsInstance<ChatEntry.Message>().map { it.message.id }.filter { it.startsWith("progress-") })
        assertTrue(entries.filterIsInstance<ChatEntry.Work>().all { group -> group.messages.all { it.tool != null } })
        assertEquals("answer", (entries.last() as ChatEntry.Message).message.id)
    }

    @Test fun userMessagesAndActualRepliesKeepSeparateWorkGroups() {
        val entries = chatEntries(listOf(text("u1", user = true), tool("a"), text("answer1"), text("u2", user = true), tool("b"), tool("c"), text("answer2")))
        assertEquals(listOf("message:u1", "work:a", "message:answer1", "message:u2", "work:b", "message:answer2"), entries.map { it.key })
    }

    @Test fun appendingAndCompletingCallsRetainsTheGroupIdentity() {
        val initial = chatEntries(listOf(tool("a", complete = false))).single()
        val updated = chatEntries(listOf(tool("a"), tool("b", complete = false))).single()
        assertEquals(initial.key, updated.key)
        assertEquals(2, (updated as ChatEntry.Work).calls.size)
        assertEquals("b", updated.calls.last().id)
    }

    @Test fun progressWithoutToolsStaysReadable() {
        val messages = listOf(text("progress", intermediate = true), text("reply"))
        assertEquals(messages, chatEntries(messages).map { (it as ChatEntry.Message).message })
        assertTrue(chatEntries(emptyList()).isEmpty())
    }

    @Test fun anAttachmentOrReplyItemReadsItsOwnBlocksAndStaysOutsideWorkGroups() {
        val file = AttachmentInfo(id = "f", name = "notes.txt", mime = "text/plain", size = 5uL, generatedImage = false)
        val reply = QuestionReplyInfo(id = "q", question = "Which?", answer = "This")
        val sent = ItemInfo(id = "sent", role = ItemRole.USER, blocks = listOf(BlockInfo.Text("Read"), BlockInfo.Attachment(file)), intermediate = false, origin = null)
        val answered = ItemInfo(id = "answered", role = ItemRole.USER, blocks = listOf(BlockInfo.QuestionReply(reply)), intermediate = false, origin = null)
        assertEquals("Read", sent.text)
        assertEquals(listOf(file), sent.attachments)
        assertEquals(listOf(reply), answered.questionReplies)
        assertEquals("", answered.text)
        assertEquals(listOf("message:sent", "work:a", "message:answered"), chatEntries(listOf(sent, tool("a"), answered)).map { it.key })
    }
}
