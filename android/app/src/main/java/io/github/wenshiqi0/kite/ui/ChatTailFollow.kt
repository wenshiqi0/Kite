package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow

/** Follow measured growth, without jumping and remeasuring the list for every token. */
@Composable
internal fun ChatTailFollow(state: LazyListState, context: String, enabled: () -> Boolean) {
    val currentEnabled by rememberUpdatedState(enabled)
    val active = enabled()
    LaunchedEffect(state, context, active) {
        if (!active) return@LaunchedEffect
        var positioned = false
        snapshotFlow {
            val layout = state.layoutInfo
            val last = layout.visibleItemsInfo.lastOrNull()
            TailLayout(
                layout.totalItemsCount,
                last?.index,
                last?.let { it.offset + it.size + layout.afterContentPadding - layout.viewportEndOffset } ?: 0,
                layout.viewportEndOffset - layout.viewportStartOffset,
                state.isScrollInProgress,
            )
        }.collect { tail ->
            // A touch can pause following before the next recomposition cancels this effect.
            if (!currentEnabled() || tail.scrolling || tail.lastIndex == null || tail.viewportSize <= 0) return@collect
            val entering = !positioned
            positioned = true
            when {
                // Seek once on entry or an explicit return, then retain the measured anchor.
                entering && tail.lastIndex < tail.count - 1 -> state.scrollToItem(tail.count - 1)
                tail.lastIndex < tail.count - 1 -> state.scrollBy((tail.overflow.coerceAtLeast(0) + tail.viewportSize).toFloat())
                tail.overflow > 0 -> state.scrollBy(tail.overflow.toFloat())
            }
        }
    }
}

private data class TailLayout(val count: Int, val lastIndex: Int?, val overflow: Int, val viewportSize: Int, val scrolling: Boolean)
