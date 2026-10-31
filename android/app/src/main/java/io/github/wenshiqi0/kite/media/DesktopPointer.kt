package io.github.wenshiqi0.kite.media

/** Local pointer hotspot in source pixels; the mouse widget is only an input control. */
class DesktopPointer(private val viewport: DesktopViewport) {
    var source: DesktopViewport.Point? = null
        private set
    private var sourceWidth = viewport.sourceWidth
    private var sourceHeight = viewport.sourceHeight
    val view: DesktopViewport.Point? get() = source?.let { viewport.viewAt(it.x, it.y) }

    fun place(x: Float, y: Float, clamp: Boolean = false): DesktopViewport.Point? {
        resizeSource()
        val point = viewport.sourceAt(x, y, clamp) ?: return null
        source = point
        return point
    }

    fun move(dx: Float, dy: Float): DesktopViewport.Point {
        resizeSource()
        val start = view ?: DesktopViewport.Point(viewport.width / 2, viewport.height / 2)
        place(start.x + dx, start.y + dy, clamp = true)
        keepVisible()
        return source!!
    }

    /** Layout/zoom/pan can move the hotspot locally, but never send remote input. */
    fun keepVisible() {
        resizeSource()
        view?.let { place(it.x.coerceIn(0f, viewport.width - .001f), it.y.coerceIn(0f, viewport.height - .001f), clamp = true) }
    }

    private fun resizeSource() {
        if (sourceWidth == viewport.sourceWidth && sourceHeight == viewport.sourceHeight) return
        source = source?.let {
            DesktopViewport.Point(it.x / sourceWidth * viewport.sourceWidth, it.y / sourceHeight * viewport.sourceHeight)
        }
        sourceWidth = viewport.sourceWidth
        sourceHeight = viewport.sourceHeight
    }

    fun clear() { source = null }
}
