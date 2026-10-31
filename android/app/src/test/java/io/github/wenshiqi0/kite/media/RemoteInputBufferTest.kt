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
    }, { sent.add(it); true }, { releases++ }, { failures++ })
    private fun advance(ms: Long) {
        now += ms
        while (true) { val next = tasks.firstOrNull { it.first <= now } ?: break; tasks.remove(next); next.second() }
    }
    private fun point(x: Int) = DesktopEvent.Position(x, 100, 1920, 1080)

    @Test fun continuousMotionSendsNothingUntilTwoHundredMillisecondsOfQuiet() {
        repeat(100) { buffer.append(point(it)); advance(20); assertTrue(sent.isEmpty()) }
        advance(179); assertTrue(sent.isEmpty())
        advance(1); assertEquals(listOf(point(99)), sent)
    }
    @Test fun clicksAndDragsRetainTheirTargetBarriers() {
        buffer.append(point(10)); buffer.append(DesktopEvent.Button(1, true))
        repeat(100) { buffer.append(point(100 + it)); advance(10) }
        buffer.append(DesktopEvent.Button(1, false))
        assertTrue(sent.isEmpty()); advance(200)
        assertEquals(listOf(point(10), DesktopEvent.Button(1, true), point(199), DesktopEvent.Button(1, false)), sent)
    }
    @Test fun wheelAndKeyboardInputKeepOneOrderedQuietWindow() {
        buffer.append(point(20)); buffer.append(DesktopEvent.Scroll(0, 20)); buffer.append(DesktopEvent.Scroll(0, 100))
        buffer.append(DesktopEvent.Key(162, true, 0)); buffer.append(DesktopEvent.Text("ab")); buffer.append(DesktopEvent.Text("你"))
        buffer.append(DesktopEvent.Key(162, false, 0)); advance(199); assertTrue(sent.isEmpty()); advance(1)
        assertEquals(listOf(point(20), DesktopEvent.Scroll(0, 120), DesktopEvent.Key(162, true, 0), DesktopEvent.Text("ab你"), DesktopEvent.Key(162, false, 0)), sent)
    }
    @Test fun cancellingDropsUnsentActionsAndAlwaysReleasesTheRemoteState() {
        buffer.append(DesktopEvent.Button(1, true)); buffer.cancel(); advance(1000)
        assertTrue(sent.isEmpty()); assertEquals(1, releases)
    }
    @Test fun wheelPositionRefreshesDoNotPreventGestureCoalescingOrRemoveClickBarriers() {
        buffer.append(point(20)); buffer.append(DesktopEvent.Scroll(0, 40))
        buffer.append(point(20)); buffer.append(DesktopEvent.Scroll(0, 80))
        buffer.append(point(20)); buffer.append(DesktopEvent.Button(1, true)); buffer.append(DesktopEvent.Button(1, false))
        advance(200)
        assertEquals(listOf(point(20), DesktopEvent.Scroll(0, 120), point(20), DesktopEvent.Button(1, true), DesktopEvent.Button(1, false)), sent)
    }
    @Test fun unchangedLayoutPositionsDoNotExtendTheQuietWindow() {
        buffer.append(point(50)); advance(150); buffer.append(point(50)); advance(50)
        assertEquals(listOf(point(50)), sent)
    }
    @Test fun physicalMotionAtAClampedEdgeStillDefersQueuedActions() {
        buffer.append(point(50)); buffer.append(DesktopEvent.Button(1, true))
        repeat(50) { advance(100); buffer.activity(); assertTrue(sent.isEmpty()) }
        advance(199); assertTrue(sent.isEmpty()); advance(1); assertFalse(sent.isEmpty())
    }
    @Test fun boundedQueueFailureDoesNotFlushAnUnfinishedGesture() {
        repeat(513) { buffer.append(DesktopEvent.Key(it, true, 0)) }
        advance(1000)
        assertTrue(sent.isEmpty()); assertEquals(1, failures); assertEquals(1, releases)
    }
}
