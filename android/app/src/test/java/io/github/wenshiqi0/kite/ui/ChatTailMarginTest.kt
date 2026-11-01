package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class ChatTailMarginTest {
    @Test fun aTouchOrSmallDragDoesNotShowLatest() {
        val margin=ChatTailMargin()
        assertFalse(margin.visible)
        margin.scrolled(120f);margin.scrolled(119f)
        assertFalse(margin.visible)
        margin.scrolled(1f);assertTrue(margin.visible)
    }
    @Test fun onlyMeasuredTailProximityCanHideTheButton() {
        val margin=ChatTailMargin();margin.scrolled(400f)
        margin.scrolled(-800f)
        assertTrue("Scroll deltas cannot prove that later rows are gone",margin.visible)
        margin.observeTail(97f);assertTrue(margin.visible)
        margin.observeTail(96f);assertFalse(margin.visible)
        margin.observeTail(180f);assertFalse(margin.visible)
        margin.observeTail(240f);assertTrue(margin.visible)
    }
    @Test fun aTallHistoricalUserReplyCannotResetTheDistanceToTheNewestReply() {
        val margin=ChatTailMargin();margin.scrolled(8000f)
        margin.observe(chatTailMeasurement(40,102,900f,600f,10f))
        assertEquals(8000f,margin.distanceDp,0f)
        margin.scrolled(-7900f)
        margin.observe(chatTailMeasurement(41,102,610f,600f,10f))
        assertTrue(margin.visible)
        margin.scrolled(-1000f)
        margin.observe(chatTailMeasurement(90,102,600f,600f,10f))
        assertTrue("Still has unmeasured history after this reply",margin.visible)
        margin.observe(chatTailMeasurement(100,102,700f,600f,10f))
        assertTrue(margin.visible)
        margin.observe(chatTailMeasurement(100,102,670f,600f,10f))
        assertFalse("Only the final content row plus spacer fits the hide margin",margin.visible)
    }
    @Test fun penultimateContentAndTheActualSpacerMeasureTheSameTail() {
        val content=chatTailMeasurement(100,102,680f,600f,10f)
        val spacer=chatTailMeasurement(101,102,696f,600f,10f)
        assertEquals(106f,content.tailDistanceDp!!,0f)
        assertEquals(content.tailDistanceDp,spacer.tailDistanceDp)
        assertNull(chatTailMeasurement(99,102,680f,600f,10f).tailDistanceDp)
        assertNull(chatTailMeasurement(100,103,680f,600f,10f).tailDistanceDp)
    }
    @Test fun aGrowingReplyCanShowTheButtonBeforeItsLaterRowsAreVisible() {
        val margin=ChatTailMargin();margin.scrolled(36f)
        margin.observe(chatTailMeasurement(499,502,1000f,600f,10f))
        assertTrue(margin.visible)
        margin.observe(chatTailMeasurement(499,502,620f,600f,10f))
        assertTrue(margin.visible)
    }
    @Test fun aTemporaryCannotScrollFlagDoesNotResumeFollowInHistory() {
        val history=chatTailMeasurement(40,102,610f,600f,10f)
        assertFalse(history.canResumeFollow(false,false))
        val tail=chatTailMeasurement(101,102,590f,600f,10f)
        assertFalse(tail.canResumeFollow(true,false))
        assertFalse(tail.canResumeFollow(false,true))
        assertTrue(tail.canResumeFollow(false,false))
    }
    @Test fun invalidTransientLayoutNeverConfirmsArrivalAtTheTail() {
        val margin=ChatTailMargin();margin.scrolled(800f)
        val invalid=chatTailMeasurement(20,0,100f,600f,10f)
        assertNull(invalid.tailDistanceDp)
        margin.observe(invalid);assertTrue(margin.visible)
        margin.reset();assertFalse(margin.visible);assertEquals(0f,margin.distanceDp,0f)
    }
}
