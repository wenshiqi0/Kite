package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.*
import org.junit.Assert.*
import org.junit.Test

class ChatTimelineStateTest {
    private fun message(i:Int)=MessageInfo(emptyList(),"m"+i,MessageKind.ASSISTANT,"message "+i,false,null)
    private fun tool(id:String)=MessageInfo(emptyList(),id,MessageKind.TOOL,"",false,ToolInfo("Bash","command","Bash","echo test","",false,false,false))
    private fun delta(revision:ULong,reset:Boolean=false,at:Int=0,remove:Int=0,items:List<MessageInfo>,context:String="host/thread")=
        ConversationDeltaInfo(context,revision,reset,listOf(ConversationEditInfo(at.toULong(),remove.toULong(),items)))
    @Test fun tenThousandMessagesStayAvailableAndTailEditsPreserveOtherRows() {
        val timeline=ChatTimelineState();timeline.prepareContext("host/thread")
        timeline.apply(delta(1uL,true,items=(0 until 10000).map(::message)))
        val first=timeline.rows[0];val middle=timeline.rows[5000]
        timeline.apply(delta(2uL,at=9999,remove=1,items=listOf(message(9999).copy(text="streamed update"))))
        assertEquals(10000,timeline.messages.size);assertEquals(10000,timeline.rows.size)
        assertSame(first,timeline.rows[0]);assertSame(middle,timeline.rows[5000])
        timeline.apply(delta(3uL,at=10000,items=listOf(message(10000))))
        assertEquals(10001,timeline.rows.size);assertSame(first,timeline.rows[0])
        assertEquals("streamed update",timeline.messages[9999].text)
    }
    @Test fun earlierDataAndNewCallsKeepTheSameExpandedWorkGroupKey() {
        val timeline=ChatTimelineState();timeline.prepareContext("host/thread")
        timeline.apply(delta(1uL,true,items=listOf(tool("call-b"))))
        val key=timeline.rows.single().key
        timeline.apply(delta(2uL,at=0,items=listOf(tool("call-a"))))
        timeline.apply(delta(3uL,at=2,items=listOf(tool("call-c"))))
        assertEquals(key,timeline.rows.single().key)
        assertEquals(3,(timeline.rows.single() as ChatEntry.Work).calls.size)
        timeline.apply(delta(4uL,at=2,remove=1,items=listOf(tool("call-c").let{it.copy(tool=it.tool!!.copy(complete=true,output="done"))})))
        assertEquals(key,timeline.rows.single().key)
        assertEquals("done",(timeline.rows.single() as ChatEntry.Work).calls.last().tool!!.output)
    }
    @Test fun changingSessionsRejectsLateDataAndNeverDisplaysThePreviousConversation() {
        val timeline=ChatTimelineState();timeline.prepareContext("host/thread")
        timeline.apply(delta(1uL,true,items=listOf(message(1))))
        timeline.prepareContext("host/other")
        assertTrue(timeline.rows.isEmpty())
        try {timeline.apply(delta(2uL,items=listOf(message(2))));fail("stale scope accepted")}catch(_:IllegalArgumentException){}
        assertTrue(timeline.messages.isEmpty())
        timeline.apply(delta(3uL,true,items=listOf(message(3)),context="host/other"))
        assertEquals("m3",timeline.messages.single().id)
    }
    @Test fun veryLongLiteralOutputKeepsAllTextWithoutOneGiantLayoutParagraph() {
        val source="odd"+"😀".repeat(9000)+"\n"+"complete output\r\n".repeat(10000)
        val chunks=literalChunks(source)
        assertEquals(source,chunks.joinToString(""))
        assertTrue(chunks.all{it.length<=1024&&!Character.isHighSurrogate(it.last())})
    }
}
