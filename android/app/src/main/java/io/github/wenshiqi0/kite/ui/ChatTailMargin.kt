package io.github.wenshiqi0.kite.ui

internal const val CHAT_ROW_SPACING_DP = 8f
internal const val CHAT_TAIL_SPACER_DP = 8f

/** Interior row overflow is only a lower bound, never a measurement of the tail. */
internal data class ChatTailMeasurement(val lowerBoundDp: Float, val tailDistanceDp: Float?) {
    fun canResumeFollow(scrolling: Boolean, canForward: Boolean): Boolean =
        !scrolling && !canForward && tailDistanceDp?.let { it <= ChatTailMargin.HIDE_DP } == true
}
internal fun chatTailMeasurement(lastIndex: Int, totalItems: Int, lastBottomDp: Float, viewportEndDp: Float, afterPaddingDp: Float): ChatTailMeasurement {
    if (totalItems <= 0 || lastIndex !in 0 until totalItems) return ChatTailMeasurement(0f, null)
    val lower = (lastBottomDp + afterPaddingDp - viewportEndDp).coerceAtLeast(0f)
    val tail = when {
        lastIndex == totalItems - 1 -> lower
        lastIndex == totalItems - 2 -> (lastBottomDp + afterPaddingDp + CHAT_ROW_SPACING_DP + CHAT_TAIL_SPACER_DP - viewportEndDp).coerceAtLeast(0f)
        else -> null
    }
    return ChatTailMeasurement(lower, tail)
}

/** Once shown, only measured tail proximity or an explicit jump can hide it. */
class ChatTailMargin {
    companion object { const val SHOW_DP=240f; const val HIDE_DP=96f }
    var distanceDp: Float = 0f
        private set
    var visible: Boolean = false
        private set
    fun scrolled(earlierDp: Float) {
        distanceDp = (distanceDp + earlierDp).coerceAtLeast(0f)
        if (distanceDp >= SHOW_DP) visible = true
    }
    internal fun observe(measurement: ChatTailMeasurement) {
        val tail = measurement.tailDistanceDp
        if (tail != null) observeTail(tail)
        else {
            distanceDp = maxOf(distanceDp, measurement.lowerBoundDp)
            if (measurement.lowerBoundDp >= SHOW_DP) visible = true
        }
    }
    fun observeTail(distance: Float) {
        distanceDp = distance.coerceAtLeast(0f)
        if (distanceDp >= SHOW_DP) visible = true
        else if (distanceDp <= HIDE_DP) visible = false
    }
    fun reset() { distanceDp = 0f; visible = false }
}
