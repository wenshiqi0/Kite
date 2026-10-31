package io.github.wenshiqi0.kite.ui

/** Local scroll geometry only; pausing follow does not itself show the affordance. */
class ChatTailMargin {
    companion object { const val SHOW_DP=120f; const val HIDE_DP=48f }
    var distanceDp: Float = 0f
        private set
    var visible: Boolean = false
        private set
    fun scrolled(earlierDp: Float) = observe(distanceDp + earlierDp)
    fun observe(distance: Float) {
        distanceDp = distance.coerceAtLeast(0f)
        if (distanceDp >= SHOW_DP) visible = true
        else if (distanceDp <= HIDE_DP) visible = false
    }
    fun reset() { distanceDp = 0f; visible = false }
}
