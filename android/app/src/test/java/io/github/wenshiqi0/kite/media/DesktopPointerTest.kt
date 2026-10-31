package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class DesktopPointerTest {
    private val viewport = DesktopViewport().apply { resize(960f, 540f) }
    private val pointer = DesktopPointer(viewport)

    @Test fun hotspotIsSeparateFromTheMouseControlAndStoredInSourcePixels() {
        pointer.place(200f, 150f)
        assertEquals(400f, pointer.source!!.x, .001f)
        assertEquals(300f, pointer.source!!.y, .001f)
        pointer.move(40f, -20f)
        assertEquals(480f, pointer.source!!.x, .001f)
        assertEquals(260f, pointer.source!!.y, .001f)
        assertEquals(240f, pointer.view!!.x, .001f)
        assertEquals(130f, pointer.view!!.y, .001f)
    }

    @Test fun zoomAndPanUseTheSameTransformForTheArrowAndInput() {
        viewport.zoomAt(3f); viewport.pan(120f, -60f)
        pointer.place(310f, 260f)
        val expected = viewport.sourceAt(370f, 230f)!!
        pointer.move(60f, -30f)
        assertEquals(expected.x, pointer.source!!.x, .001f)
        assertEquals(expected.y, pointer.source!!.y, .001f)
        assertEquals(370f, pointer.view!!.x, .001f)
        assertEquals(230f, pointer.view!!.y, .001f)
    }

    @Test fun reachingTheWidgetEdgeDoesNotLimitTheSourcePointer() {
        pointer.place(400f, 250f)
        repeat(30) { pointer.move(50f, 50f) }
        assertEquals(1919f, pointer.source!!.x, .001f)
        assertEquals(1079f, pointer.source!!.y, .001f)
        repeat(30) { pointer.move(-50f, -50f) }
        assertEquals(0f, pointer.source!!.x, .001f)
        assertEquals(0f, pointer.source!!.y, .001f)
    }

    @Test fun letterboxAndSourceEdgesClampWithoutCreatingInvalidCoordinates() {
        viewport.resize(960f, 960f)
        pointer.place(400f, 50f, clamp = true)
        assertEquals(0f, pointer.source!!.y, .001f)
        assertEquals(210f, pointer.view!!.y, .001f)
        pointer.move(-2000f, 2000f)
        assertEquals(0f, pointer.source!!.x, .001f)
        assertEquals(1079f, pointer.source!!.y, .001f)
    }

    @Test fun layoutChangesRetainLogicalPositionAndBringAnOffscreenHotspotIntoView() {
        pointer.place(800f, 450f)
        viewport.resize(540f, 960f); pointer.keepVisible()
        assertEquals(1600f, pointer.source!!.x, .001f)
        assertEquals(900f, pointer.source!!.y, .001f)
        viewport.zoomAt(4f); pointer.keepVisible()
        val view = pointer.view!!
        assertTrue(view.x in 0f..540f && view.y in 0f..960f)
        val mapped = viewport.sourceAt(view.x, view.y, clamp = true)!!
        assertEquals(mapped.x, pointer.source!!.x, .001f)
        assertEquals(mapped.y, pointer.source!!.y, .001f)
    }

    @Test fun changingSourceResolutionPreservesNormalizedPositionAndClearDropsOldTarget() {
        pointer.place(240f, 135f)
        viewport.resize(960f, 540f, 3840f, 2160f); pointer.keepVisible()
        assertEquals(960f, pointer.source!!.x, .001f)
        assertEquals(540f, pointer.source!!.y, .001f)
        assertEquals(240f, pointer.view!!.x, .001f)
        pointer.clear()
        assertNull(pointer.source); assertNull(pointer.view)
    }
}
