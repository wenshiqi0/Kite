package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class VideoPerformanceTest {
    @Test fun transientInputPressureDoesNotRequireDroppingAReferenceFrame() {
        val budget=VideoFeedBudget()
        assertFalse(budget.expired(0,5_000_000))
        assertFalse(budget.expired(0,49_999_999))
        assertTrue(budget.expired(0,50_000_000))
    }
    @Test fun recoveryRequestsAreCoalescedDuringASustainedStall() {
        val budget=VideoFeedBudget()
        assertTrue(budget.requestIdr(0))
        repeat(249){assertFalse(budget.requestIdr((it+1)*1_000_000L))}
        assertTrue(budget.requestIdr(250_000_000))
    }
    @Test fun fpsAndLatencyDescribeRecentFramesRatherThanRequestedFrameRate() {
        var now=0L;val performance=VideoPerformance{now};performance.configured("owned-hardware",true)
        repeat(60){i->
            val at=i*16_666_667L;now=at;performance.received(i.toLong(),1,at)
            now=at+2_000_000;performance.submitted(at)
            now=at+10_000_000;performance.rendered(i.toLong(),at+8_000_000)
        }
        now=1_000_000_000;val value=performance.snapshot()
        assertEquals(60.0,value.receivedFps,.01);assertEquals(60.0,value.renderedFps,.01)
        assertEquals(2.0,value.queueP95Ms,.001);assertEquals(8.0,value.renderP95Ms,.001);assertEquals(2.0,value.callbackP95Ms,.001)
        assertTrue(value.hardware)
        now=4_000_000_000;assertEquals(0.0,performance.snapshot().renderedFps,0.0)
    }
    @Test fun samplingStaysBoundedAndReportsPressureWithoutFrameContents() {
        var now=0L;val performance=VideoPerformance{now}
        repeat(10000){now+=16_000_000;performance.received(it.toLong(),3,now);performance.submitted(now-1_000_000);performance.rendered(it.toLong(),now)}
        performance.inputWait();performance.inputTimeout();performance.rejected();performance.superseded();performance.idr()
        val value=performance.snapshot();assertEquals(10000L,value.received);assertTrue(value.renderedFps<=63)
        assertEquals(1L,value.inputTimeouts);assertEquals(1L,value.idrRequests);assertEquals(3,value.maxQueued)
    }
}
