package io.github.wenshiqi0.kite.media

import kotlin.math.max
import kotlin.math.min

/** Source desktop pixels are the single coordinate system for rendering and input. */
class DesktopViewport {
    data class Point(val x: Float, val y: Float)
    var sourceWidth = 1920f; private set
    var sourceHeight = 1080f; private set
    var width = 1f; private set
    var height = 1f; private set
    var zoom = 1f; private set
    var centerX = sourceWidth / 2; private set
    var centerY = sourceHeight / 2; private set
    val fitScale: Float get() = min(width / sourceWidth, height / sourceHeight)
    val scale: Float get() = fitScale * zoom
    val left: Float get() = width / 2 - centerX * scale
    val top: Float get() = height / 2 - centerY * scale

    fun resize(w: Float, h: Float, sourceW: Float = sourceWidth, sourceH: Float = sourceHeight) {
        if (sourceW > 0 && sourceH > 0 && (sourceW != sourceWidth || sourceH != sourceHeight)) {
            centerX = centerX / sourceWidth * sourceW
            centerY = centerY / sourceHeight * sourceH
            sourceWidth = sourceW; sourceHeight = sourceH
        }
        width = max(1f, w); height = max(1f, h)
        clampCenter()
    }

    fun sourceAt(x: Float, y: Float, clamp: Boolean = false): Point? {
        val sx = (x - left) / scale
        val sy = (y - top) / scale
        if (!clamp && (sx < 0 || sy < 0 || sx >= sourceWidth || sy >= sourceHeight)) return null
        return Point(sx.coerceIn(0f, sourceWidth - 1), sy.coerceIn(0f, sourceHeight - 1))
    }

    fun viewAt(x: Float, y: Float) = Point(left + x * scale, top + y * scale)
    fun sourceDelta(dx: Float, dy: Float) = Point(dx / scale, dy / scale)

    fun zoomAt(value: Float, focusX: Float = width / 2, focusY: Float = height / 2) {
        val beforeX = (focusX - left) / scale
        val beforeY = (focusY - top) / scale
        zoom = value.coerceIn(1f, 8f)
        centerX = beforeX - (focusX - width / 2) / scale
        centerY = beforeY - (focusY - height / 2) / scale
        clampCenter()
    }

    fun pan(dx: Float, dy: Float) {
        centerX -= dx / scale; centerY -= dy / scale
        clampCenter()
    }

    fun fit() { zoom = 1f; centerX = sourceWidth / 2; centerY = sourceHeight / 2 }
    private fun clampCenter() {
        val halfW = min(sourceWidth / 2, width / (2 * scale))
        val halfH = min(sourceHeight / 2, height / (2 * scale))
        centerX = centerX.coerceIn(halfW, sourceWidth - halfW)
        centerY = centerY.coerceIn(halfH, sourceHeight - halfH)
    }
}
