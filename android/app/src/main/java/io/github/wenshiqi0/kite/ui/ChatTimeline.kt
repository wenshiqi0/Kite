package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.MessageInfo
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue

/** Presentation projection; the original Rust conversation and call IDs stay intact. */
sealed interface ChatEntry {
    val key: String
    data class Message(val message: MessageInfo) : ChatEntry {
        override val key = "message:${message.id}"
    }
    data class Work(val messages: List<MessageInfo>, val stableKey:String?=null) : ChatEntry {
        val calls = messages.filter { it.tool != null }
        override val key = stableKey ?: ("work:"+messages.first { it.tool != null }.id)
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

/** Complete presentation data. Compose virtualizes rows; this store never evicts them. */
class ChatTimelineState {
    private val messageItems = androidx.compose.runtime.mutableStateListOf<MessageInfo>()
    val messages:List<MessageInfo> get()=messageItems
    private val rowItems = androidx.compose.runtime.mutableStateListOf<ChatEntry>()
    val rows:List<ChatEntry> get()=rowItems
    var context by androidx.compose.runtime.mutableStateOf("")
        private set
    var revision by androidx.compose.runtime.mutableStateOf(0uL)
        private set
    var appliedContext = ""
        private set
    private val rowByMessage = mutableMapOf<String,Int>()

    fun prepareContext(next:String) {
        if(context==next)return
        androidx.compose.runtime.snapshots.Snapshot.withMutableSnapshot {
            context=next;appliedContext="";revision=0uL
            messageItems.clear();rowItems.clear();rowByMessage.clear()
        }
    }
    fun apply(delta:io.github.wenshiqi0.kite.core.ConversationDeltaInfo) {
        require(delta.context==context)
        if(!delta.reset && delta.revision<revision)return
        androidx.compose.runtime.snapshots.Snapshot.withMutableSnapshot {
            if(delta.reset){messageItems.clear();rowItems.clear();rowByMessage.clear()}
            for(edit in delta.edits)splice(edit.at.toInt(),edit.remove.toInt(),edit.items)
            revision=delta.revision;appliedContext=delta.context
        }
    }
    private fun working(m:MessageInfo)=m.tool!=null||m.intermediate
    private fun rowMessages(row:ChatEntry)=if(row is ChatEntry.Message)listOf(row.message)else(row as ChatEntry.Work).messages
    private fun indexRows(start:Int=0) {
        if(start==0)rowByMessage.clear()
        for(i in start until rows.size)for(m in rowMessages(rows[i]))rowByMessage[m.id]=i
    }
    private fun preserveWorkKeys(projected:List<ChatEntry>,old:List<ChatEntry>):List<ChatEntry> {
        val anchors=old.filterIsInstance<ChatEntry.Work>().associateBy {it.key.removePrefix("work:")}
        return projected.map {row->
            if(row !is ChatEntry.Work)row
            else row.messages.firstNotNullOfOrNull {anchors[it.id]}?.let{row.copy(stableKey=it.key)}?:row
        }
    }
    private fun replaceRows(next:List<ChatEntry>) {
        var head=0
        while(head<minOf(rows.size,next.size)&&rows[head]==next[head])head++
        var tail=0
        while(tail<minOf(rows.size-head,next.size-head)&&rows[rows.lastIndex-tail]==next[next.lastIndex-tail])tail++
        repeat(rows.size-head-tail){rowItems.removeAt(head)}
        rowItems.addAll(head,next.subList(head,next.size-tail))
        indexRows()
    }
    private fun splice(at:Int,remove:Int,added:List<MessageInfo>) {
        require(at>=0&&remove>=0&&at+remove<=messages.size)
        val previousSize=messages.size
        if(remove==1&&added.size==1&&messages[at].id==added[0].id&&working(messages[at])==working(added[0])) {
            val rowIndex=checkNotNull(rowByMessage[messages[at].id])
            messageItems[at]=added[0]
            val row=rows[rowIndex]
            rowItems[rowIndex]=if(row is ChatEntry.Message)ChatEntry.Message(added[0])else{
                row as ChatEntry.Work
                row.copy(messages=row.messages.map{if(it.id==added[0].id)added[0]else it})
            }
            return
        }
        repeat(remove){messageItems.removeAt(at)}
        messageItems.addAll(at,added)
        if(at==previousSize&&remove==0) {
            var start=rows.size
            while(start>0){
                val row=rows[start-1]
                if(row is ChatEntry.Work || row is ChatEntry.Message&&row.message.intermediate)start--
                else break
            }
            val old=rows.subList(start,rows.size).toList()
            val suffix=old.flatMap(::rowMessages)+added
            val next=preserveWorkKeys(chatEntries(suffix),old)
            for(row in old)for(m in rowMessages(row))rowByMessage.remove(m.id)
            repeat(rows.size-start){rowItems.removeAt(start)}
            rowItems.addAll(next);indexRows(start)
        } else {
            val old=rows.toList()
            replaceRows(preserveWorkKeys(chatEntries(messages),old))
        }
    }
}
