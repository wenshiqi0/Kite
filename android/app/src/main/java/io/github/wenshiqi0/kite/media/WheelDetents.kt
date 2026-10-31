package io.github.wenshiqi0.kite.media

import kotlin.math.abs
import kotlin.math.sign

/** Tactile detents follow accepted scroll units, with a ceiling on vibration frequency. */
class WheelDetents {
    private var remainder = 0
    private var lastTick: Long? = null

    fun accept(units: Int, now: Long): Boolean {
        if (units == 0) return false
        if (remainder.sign != units.sign) remainder = 0
        remainder += units
        if (abs(remainder) < 120) return false
        remainder %= 120
        val previous = lastTick
        if (previous != null && now - previous < 40) return false
        lastTick = now
        return true
    }

    fun reset() { remainder = 0; lastTick = null }
}
