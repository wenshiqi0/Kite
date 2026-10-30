package io.github.wenshiqi0.kite.media

/** Relative pointer/wheel units do not depend on the local video zoom. */
class PointerMotion {
    private var x = 0f; private var y = 0f; private var wheel = 0f
    data class Delta(val x: Int, val y: Int)
    fun move(dx: Float, dy: Float, density: Float, sensitivity: Float = 2.2f): Delta {
        x += dx / density * sensitivity; y += dy / density * sensitivity
        val result = Delta(x.toInt(), y.toInt()); x -= result.x; y -= result.y
        return result
    }
    fun scroll(dy: Float, density: Float): Int {
        wheel -= dy / density * 120f / 36f
        val result = wheel.toInt(); wheel -= result; return result
    }
    fun reset() { x = 0f; y = 0f; wheel = 0f }
}
