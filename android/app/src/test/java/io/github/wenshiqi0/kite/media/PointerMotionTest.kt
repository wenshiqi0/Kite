package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class PointerMotionTest {
    @Test fun fractional_moves_accumulate_and_do_not_depend_on_viewport_zoom() {
        val p=PointerMotion();var total=0
        repeat(100){total+=p.move(.1f,0f,2f,2f).x}
        assertEquals(10,total)
        assertEquals(PointerMotion.Delta(20,-10),p.move(20f,-10f,2f,2f))
    }
    @Test fun wheel_has_continuous_direction_and_reset_discards_only_fractional_motion() {
        val p=PointerMotion()
        assertEquals(-120,p.scroll(72f,2f));assertEquals(120,p.scroll(-72f,2f))
        p.move(.1f,.1f,2f);p.reset()
        assertEquals(PointerMotion.Delta(0,0),p.move(.1f,.1f,2f))
    }
}
