package io.github.wenshiqi0.kite.media

/** The arrow is fixed to the mouse controls in view pixels; source pixels are derived. */
class DesktopPointer(private val viewport: DesktopViewport) {
    var view: DesktopViewport.Point? = null
        private set
    val source: DesktopViewport.Point? get() = view?.let { viewport.sourceAt(it.x, it.y, clamp = true) }

    fun place(x: Float, y: Float, clamp: Boolean = false): DesktopViewport.Point? {
        val point = viewport.sourceAt(x, y, clamp) ?: return null
        view = DesktopViewport.Point(x, y)
        return point
    }

    fun move(dx: Float, dy: Float): DesktopViewport.Point {
        val start = view ?: DesktopViewport.Point(viewport.width / 2, viewport.height / 2)
        place(start.x + dx, start.y + dy, clamp = true)
        return source!!
    }
    /** Keep source edges reachable without separating the arrow from its controls. */
    fun keepDesktopUnderHotspot() {
        val anchor = view ?: return
        val point = source ?: return
        val projected = viewport.viewAt(point.x, point.y)
        if (kotlin.math.abs(anchor.x - projected.x) > .01f || kotlin.math.abs(anchor.y - projected.y) > .01f) {
            viewport.pan(anchor.x - projected.x, anchor.y - projected.y)
        }
    }
    fun clear() { view = null }
}
