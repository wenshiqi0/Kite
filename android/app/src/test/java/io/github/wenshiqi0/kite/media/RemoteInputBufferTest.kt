package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class RemoteInputBufferTest {
    private var now = 0L
    private val tasks = mutableListOf<Pair<Long, () -> Unit>>()
    private val sent = mutableListOf<DesktopEvent>()
    private var releases = 0
    private var failures = 0
    private val buffer = RemoteInputBuffer({ delay, action ->
        val task = now + delay to action; tasks.add(task)
        val cancel: () -> Unit = { tasks.remove(task) }; cancel
    }, { sent.add(it); true }, { releases++ }, { failures++ }, { now })
    private fun advance(ms: Long) {
        now += ms
        while (true) { val next = tasks.firstOrNull { it.first <= now } ?: break; tasks.remove(next); next.second() }
    }
    private fun point(x: Int) = DesktopEvent.Position(x, 100, 1920, 1080)

    @Test fun continuousMotionSendsDuringTheGestureAtFixedFrameDeadlines() {
        repeat(100) { buffer.append(point(it)); advance(8) }
        assertEquals(50,sent.size)
        assertEquals(point(99),sent.last())
        assertEquals(point(1),sent.first())
    }
    @Test fun clicksAndDragsFlushTargetsAndKeepButtonBarriersImmediate() {
        buffer.append(point(10)); buffer.append(DesktopEvent.Button(1, true))
        assertEquals(listOf(point(10),DesktopEvent.Button(1,true)),sent)
        buffer.append(point(100));advance(8);buffer.append(point(101));advance(8)
        buffer.append(point(200));buffer.append(DesktopEvent.Button(1,false))
        assertEquals(listOf(point(10),DesktopEvent.Button(1,true),point(101),point(200),DesktopEvent.Button(1,false)),sent)
    }
    @Test fun wheelSamplingKeepsTheSameHotspotAndAggregatesWithinOneFrame() {
        buffer.append(point(20));buffer.append(DesktopEvent.Scroll(0,40));advance(8)
        buffer.append(point(20));buffer.append(DesktopEvent.Scroll(0,80));advance(7);assertTrue(sent.isEmpty());advance(1)
        assertEquals(listOf(point(20),DesktopEvent.Scroll(0,120)),sent)
    }
    @Test fun continuousTypingHasAMaximumTwoHundredMillisecondBatch() {
        repeat(30){buffer.append(DesktopEvent.Text("a"));advance(20)}
        assertEquals(listOf(DesktopEvent.Text("a".repeat(10)),DesktopEvent.Text("a".repeat(10)),DesktopEvent.Text("a".repeat(10))),sent)
    }
    @Test fun pointerSamplingCannotExtendAnEarlierTextDeadline() {
        buffer.append(DesktopEvent.Text("你"));advance(195);buffer.append(point(10));advance(5)
        assertEquals(listOf(DesktopEvent.Text("你"),point(10)),sent)
    }
    @Test fun keysFlushPendingTextBeforeTheModifierOrShortcut() {
        buffer.append(DesktopEvent.Text("ab"));buffer.append(DesktopEvent.Text("你"));buffer.append(DesktopEvent.Key(162,true,0))
        assertEquals(listOf(DesktopEvent.Text("ab你"),DesktopEvent.Key(162,true,0)),sent)
    }
    @Test fun cancellingDropsUnsentMotionAndReleasesAlreadySentState() {
        buffer.append(DesktopEvent.Button(1,true));buffer.append(point(99));buffer.cancel();advance(1000)
        assertEquals(listOf(DesktopEvent.Button(1,true)),sent);assertEquals(1,releases)
    }
    @Test fun duplicateLayoutPositionsDoNotCreateMoreTimersOrPackets() {
        buffer.append(point(50));advance(8);buffer.append(point(50));advance(8)
        assertEquals(listOf(point(50)),sent)
    }
    @Test fun boundedQueueFailureDoesNotReplayUnsentActions() {
        for(i in 0..600){if(!buffer.append(if(i%2==0)DesktopEvent.Text("a")else point(i)))break}
        advance(1000);assertTrue(sent.isEmpty());assertEquals(1,failures);assertEquals(1,releases)
    }
}
