package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class DesktopViewportTest {
    private fun viewport() = DesktopViewport().apply { resize(960f, 540f, 1920f, 1080f) }
    @Test fun fitMapsPhoneCenterAndCornersIntoSourcePixels() {
        val v = viewport()
        assertEquals(960f, v.sourceAt(480f, 270f)!!.x, .001f)
        assertEquals(540f, v.sourceAt(480f, 270f)!!.y, .001f)
        assertEquals(0f, v.sourceAt(0f, 0f)!!.x, .001f)
        assertEquals(1918f, v.sourceAt(959f, 539f)!!.x, .001f)
    }
    @Test fun letterboxDoesNotGenerateRemoteClicks() {
        val v = viewport().apply { resize(960f, 960f) }
        assertNull(v.sourceAt(480f, 100f))
        assertEquals(0f, v.sourceAt(480f, 210f)!!.y, .001f)
        assertNull(v.sourceAt(480f, 800f))
    }
    @Test fun zoomPreservesTheSourcePointUnderThePinchFocus() {
        val v = viewport()
        val before = v.sourceAt(300f, 210f)!!
        v.zoomAt(2f, 300f, 210f)
        assertEquals(before.x, v.sourceAt(300f, 210f)!!.x, .001f)
        assertEquals(before.y, v.sourceAt(300f, 210f)!!.y, .001f)
    }
    @Test fun zoomAndPanMapBackToTheSameRemoteTarget() {
        val v = viewport().apply { zoomAt(3f); pan(180f, -90f) }
        val target = v.viewAt(1000f, 600f)
        val result = v.sourceAt(target.x, target.y)!!
        assertEquals(1000f, result.x, .001f)
        assertEquals(600f, result.y, .001f)
        // At 3x, 150 phone pixels span 100 source pixels.
        assertEquals(100f, v.sourceDelta(150f, 0f).x, .001f)
    }
    @Test fun orientationChangesKeepDesktopResolutionAndZoom() {
        val v = viewport().apply { zoomAt(2f); resize(540f, 960f) }
        assertEquals(1920f, v.sourceWidth, .001f)
        assertEquals(1080f, v.sourceHeight, .001f)
        assertEquals(2f, v.zoom, .001f)
        assertEquals(960f, v.sourceAt(270f, 480f)!!.x, .001f)
    }
    @Test fun extremePanCannotLoseTheDesktopAndFitRestoresIt() {
        val v = viewport().apply { zoomAt(2f); pan(100000f, 100000f) }
        assertEquals(0f, v.sourceAt(0f, 0f)!!.x, .001f)
        assertEquals(0f, v.sourceAt(0f, 0f)!!.y, .001f)
        v.fit()
        assertEquals(1f, v.zoom, .001f)
        assertEquals(960f, v.centerX, .001f)
        assertEquals(540f, v.centerY, .001f)
    }
    @Test fun dragCanClampAtSourceEdgeWithoutChangingCoordinates() {
        val v = viewport()
        assertNull(v.sourceAt(-50f, 50f))
        assertEquals(0f, v.sourceAt(-50f, 50f, clamp = true)!!.x)
        assertEquals(1919f, v.sourceAt(2000f, 50f, clamp = true)!!.x)
    }
}
