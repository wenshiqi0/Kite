package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class WheelDetentsTest {
    @Test fun fractionalScrollWaitsForAWholeTactileDetent() {
        val wheel = WheelDetents()
        repeat(11) { assertFalse(wheel.accept(10, it * 50L)) }
        assertTrue(wheel.accept(10, 600))
        assertFalse(wheel.accept(0, 700))
    }

    @Test fun reversingDirectionDoesNotReuseAnUnfinishedForwardDetent() {
        val wheel = WheelDetents()
        assertFalse(wheel.accept(110, 100))
        assertFalse(wheel.accept(-60, 200))
        assertTrue(wheel.accept(-60, 300))
    }

    @Test fun fastScrollCannotBuzzAtThePointerSamplingRate() {
        val wheel = WheelDetents()
        assertTrue(wheel.accept(120, 100))
        assertFalse(wheel.accept(120, 110))
        assertFalse(wheel.accept(240, 120))
        assertTrue(wheel.accept(120, 150))
        assertFalse(wheel.accept(1, 200))
    }

    @Test fun cancellingAndStartingAgainDropsTheOldPartialDetent() {
        val wheel = WheelDetents()
        assertFalse(wheel.accept(100, 100))
        wheel.reset()
        assertFalse(wheel.accept(20, 200))
        assertTrue(wheel.accept(100, 250))
    }
}
