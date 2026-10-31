package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class ChatTailMarginTest {
    @Test fun aTouchOrSmallDragDoesNotShowLatest() {
        val margin=ChatTailMargin()
        assertFalse(margin.visible)
        margin.scrolled(30f);margin.scrolled(60f)
        assertFalse(margin.visible)
        margin.scrolled(30f);assertTrue(margin.visible)
    }
    @Test fun returningNearTheTailUsesHysteresis() {
        val margin=ChatTailMargin();margin.scrolled(140f)
        margin.scrolled(-40f);assertTrue(margin.visible)
        margin.scrolled(-51f);assertTrue(margin.visible)
        margin.scrolled(-1f);assertFalse(margin.visible)
        margin.scrolled(10f);assertFalse(margin.visible)
    }
    @Test fun measuredTailGapCorrectsAChangedMessageOrLayout() {
        val margin=ChatTailMargin();margin.scrolled(20f)
        margin.observe(180f);assertTrue(margin.visible)
        margin.observe(24f);assertFalse(margin.visible)
        margin.scrolled(-100f);assertEquals(0f,margin.distanceDp,0f)
    }
    @Test fun returningToLatestOrChangingConversationResetsTheAffordance() {
        val margin=ChatTailMargin();margin.scrolled(900f);margin.reset()
        assertFalse(margin.visible);assertEquals(0f,margin.distanceDp,0f)
    }
}
