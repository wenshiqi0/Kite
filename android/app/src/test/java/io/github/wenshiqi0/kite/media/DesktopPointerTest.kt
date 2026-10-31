package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class DesktopPointerTest {
    private val viewport = DesktopViewport().apply { resize(960f, 540f) }
    private val pointer = DesktopPointer(viewport)

    @Test fun screenHotspotDoesNotDriftWithZoomAndPan() {
        pointer.place(300f, 200f)
        val view = pointer.view!!
        val before = pointer.source!!
        viewport.zoomAt(3f); viewport.pan(100f, -60f)
        assertEquals(view, pointer.view)
        assertNotEquals(before, pointer.source)
        val source = viewport.sourceAt(view.x, view.y, clamp = true)!!
        assertEquals(source, pointer.source)
    }

    @Test fun movingTheControlMovesItsViewHotspotByTheSameDelta() {
        pointer.place(200f, 150f)
        pointer.move(40f, -20f)
        assertEquals(240f, pointer.view!!.x, .001f)
        assertEquals(130f, pointer.view!!.y, .001f)
        assertEquals(480f, pointer.source!!.x, .001f)
        assertEquals(260f, pointer.source!!.y, .001f)
    }

    @Test fun resizeAndSourceResolutionChangesReprojectWithoutMovingTheArrow() {
        pointer.place(240f, 135f)
        val before = pointer.view
        viewport.resize(540f, 960f, 3840f, 2160f)
        assertEquals(before, pointer.view)
        assertEquals(viewport.sourceAt(240f, 135f, true), pointer.source)
    }

    @Test fun edgeCoordinatesClampInputWithoutMovingTheRenderedHotspot() {
        pointer.place(-50f, 600f, clamp = true)
        assertEquals(DesktopViewport.Point(-50f, 600f), pointer.view)
        assertEquals(0f, pointer.source!!.x, .001f)
        assertEquals(1079f, pointer.source!!.y, .001f)
    }

    @Test fun inputGutterLetsAllSourceEdgesTravelUnderAFixedHotspot() {
        viewport.inputGutter(400f)
        pointer.place(650f, 350f)
        viewport.pan(-400f, -400f)
        assertEquals(1919f, pointer.source!!.x, .001f)
        assertEquals(1079f, pointer.source!!.y, .001f)
        assertEquals(DesktopViewport.Point(650f, 350f), pointer.view)
    }

    @Test fun clearingATargetDropsItsOldPointer() {
        pointer.place(240f, 135f); pointer.clear()
        assertNull(pointer.source); assertNull(pointer.view)
    }
    @Test fun letterboxEdgesMoveUnderTheAnchorInsteadOfClampingOnlyTheInput() {
        viewport.inputGutter(400f); viewport.resize(960f, 960f)
        pointer.place(20f, 100f, clamp = true)
        pointer.keepDesktopUnderHotspot()
        val p = pointer.source!!; val drawn = viewport.viewAt(p.x, p.y)
        assertEquals(20f, drawn.x, .01f); assertEquals(100f, drawn.y, .01f)
        assertEquals(DesktopViewport.Point(20f, 100f), pointer.view)
    }
}
