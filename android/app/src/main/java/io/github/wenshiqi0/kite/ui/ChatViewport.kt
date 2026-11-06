package io.github.wenshiqi0.kite.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints

/** Resolve viewport resizing and its tail anchor before drawing either change. */
@Composable
internal fun ChatViewport(
    state: LazyListState,
    context: String,
    following: () -> Boolean,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val previous=remember(state,context){ViewportMeasure()}
    val currentFollowing=rememberUpdatedState(following)
    Box(modifier.layout { measurable,constraints ->
        val resized=previous.constraints?.let{it.maxHeight!=constraints.maxHeight || it.maxWidth!=constraints.maxWidth}==true
        previous.constraints=constraints
        // Scroll state must not subscribe measurement to the layout it produces.
        if(resized && constraints.maxHeight>0) Snapshot.withoutReadObservation {
            if(currentFollowing.value() && !state.isScrollInProgress) {
                val last=state.layoutInfo.totalItemsCount-1
                if(last>=0) state.requestScrollToItem(last)
            }
        }
        val placeable=measurable.measure(constraints)
        layout(placeable.width,placeable.height){placeable.place(0,0)}
    },content=content)
}

private class ViewportMeasure { var constraints: Constraints?=null }
