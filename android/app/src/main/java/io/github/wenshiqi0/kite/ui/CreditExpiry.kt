package io.github.wenshiqi0.kite.ui

/** Pure local countdown; never changes the server's available-card count. */
internal data class CreditTimeLeft(val days: Long, val hours: Long, val minutes: Long, val expired: Boolean)
internal fun creditTimeLeft(expiresAt: Long, nowSeconds: Long): CreditTimeLeft {
    val seconds = (expiresAt - nowSeconds).coerceAtLeast(0L)
    return CreditTimeLeft(seconds / 86400, seconds % 86400 / 3600, seconds % 3600 / 60, expiresAt <= nowSeconds)
}
