package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.MessageInfo

/** Presentation projection; the original Rust conversation and call IDs stay intact. */
sealed interface ChatEntry {
    val key: String
    data class Message(val message: MessageInfo) : ChatEntry {
        override val key = "message:${message.id}"
    }
    data class Work(val messages: List<MessageInfo>) : ChatEntry {
        val calls get() = messages.filter { it.tool != null }
        override val key = "work:${messages.first { it.tool != null }.id}"
    }
}

fun chatEntries(messages: List<MessageInfo>): List<ChatEntry> = buildList {
    val work = mutableListOf<MessageInfo>()
    fun flush() {
        if (work.any { it.tool != null }) add(ChatEntry.Work(work.toList()))
        else work.forEach { add(ChatEntry.Message(it)) }
        work.clear()
    }
    messages.forEach { message ->
        if (message.tool != null || message.intermediate) work.add(message)
        else { flush(); add(ChatEntry.Message(message)) }
    }
    flush()
}
