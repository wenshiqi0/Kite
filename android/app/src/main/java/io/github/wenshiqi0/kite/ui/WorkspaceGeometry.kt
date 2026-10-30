package io.github.wenshiqi0.kite.ui

/** Layout units are dp; video resolution and input coordinates stay in source pixels. */
object WorkspaceGeometry {
    fun padHeight(usableHeight: Float): Float = if (usableHeight < 640f) 144f else 176f
    fun splitVideoHeight(width: Float, usableHeight: Float, aspect: Float, pad: Float, footer: Float, adjustment: Float): Float {
        val preferred=width/aspect.coerceIn(.1f,10f)
        val budget=(usableHeight-48f-48f-pad-footer-144f).coerceAtLeast(64f)
        return (preferred+adjustment).coerceIn(64f,budget)
    }
}
