package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.*
import org.junit.Assert.*
import org.junit.Test

class ChatTimelineTest {
    private fun text(id: String, user: Boolean = false, intermediate: Boolean = false) = MessageInfo(id = id, text = id, kind = if (user) MessageKind.USER else MessageKind.ASSISTANT, tool = null, attachments = emptyList(), intermediate = intermediate)
    private fun tool(id: String, complete: Boolean = true) = MessageInfo(id = id, text = "", kind = MessageKind.TOOL, attachments = emptyList(), intermediate = false, tool = ToolInfo(label = "Bash", summary = id, name = "exec_command", arguments = id, output = if (complete) "ok" else "", complete = complete, failed = false, computer = false))

    @Test fun longToolRunsWithProgressBecomeOneBoundedWorkEntry() {
        val calls = (1..80).flatMap { listOf(text("progress-$it", intermediate = true), tool("tool-$it")) }
        val entries = chatEntries(listOf(text("user", user = true)) + calls + text("answer"))
        assertEquals(3, entries.size)
        val work = entries[1] as ChatEntry.Work
        assertEquals(80, work.calls.size)
        assertEquals(calls.map { it.id }, work.messages.map { it.id })
        assertEquals("answer", (entries.last() as ChatEntry.Message).message.id)
    }

    @Test fun userMessagesAndActualRepliesKeepSeparateWorkGroups() {
        val entries = chatEntries(listOf(text("u1", user = true), tool("a"), text("answer1"), text("u2", user = true), tool("b"), tool("c"), text("answer2")))
        assertEquals(listOf("message:u1", "work:a", "message:answer1", "message:u2", "work:b", "message:answer2"), entries.map { it.key })
    }

    @Test fun appendingAndCompletingCallsRetainsTheGroupIdentity() {
        val initial = chatEntries(listOf(tool("a", complete = false))).single()
        val updated = chatEntries(listOf(tool("a"), text("working", intermediate = true), tool("b", complete = false))).single()
        assertEquals(initial.key, updated.key)
        assertEquals(2, (updated as ChatEntry.Work).calls.size)
        assertEquals("b", updated.calls.last().id)
    }

    @Test fun progressWithoutToolsStaysReadable() {
        val messages = listOf(text("progress", intermediate = true), text("reply"))
        assertEquals(messages, chatEntries(messages).map { (it as ChatEntry.Message).message })
        assertTrue(chatEntries(emptyList()).isEmpty())
    }
}
