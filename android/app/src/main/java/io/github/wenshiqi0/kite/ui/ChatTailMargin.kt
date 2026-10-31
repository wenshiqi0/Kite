package io.github.wenshiqi0.kite.ui

/** Local scroll geometry only; pausing follow does not itself show the affordance. */
class ChatTailMargin {
    var distanceDp: Float = 0f
        private set
    var visible: Boolean = false
        private set
    fun scrolled(earlierDp: Float) = observe(distanceDp + earlierDp)
    fun observe(distance: Float) {
        distanceDp = distance.coerceAtLeast(0f)
        if (distanceDp >= 120f) visible = true
        else if (distanceDp <= 48f) visible = false
    }
    fun reset() { distanceDp = 0f; visible = false }
}
