package io.github.wenshiqi0.kite.ui

import android.os.SystemClock
import android.view.HapticFeedbackConstants
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.semantics
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.media.PointerMotion
import io.github.wenshiqi0.kite.media.WheelDetents
import io.github.wenshiqi0.kite.media.DesktopInput
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import kotlinx.coroutines.delay

@Composable
fun MouseWheel(vm: HostCheckViewModel, enabled: Boolean, modifier: Modifier = Modifier, beforeScroll: () -> Boolean = { true }) {
    WheelControl(vm.target, enabled, vm::scroll, modifier, beforeScroll, activity = { DesktopInput.activity(vm.target) })
}

/** Tap either half for a notch, or drag for continuous scroll. No pointer button is synthesized. */
@Composable
internal fun WheelControl(
    target: Long, enabled: Boolean, scroll: (Int) -> Boolean, modifier: Modifier = Modifier,
    beforeScroll: () -> Boolean = { true }, feedback: ((Int) -> Unit)? = null, activity: () -> Unit = {},
) {
    val view = LocalView.current
    val density = LocalDensity.current.density
    val label = stringResource(R.string.mouse_wheel)
    val up = stringResource(R.string.wheel_up)
    val down = stringResource(R.string.wheel_down)
    val pressedLabel = stringResource(R.string.wheel_pressed)
    val readyLabel = stringResource(R.string.wheel_ready)
    val currentScroll by rememberUpdatedState(scroll)
    val currentBefore by rememberUpdatedState(beforeScroll)
    val currentFeedback by rememberUpdatedState(feedback)
    val currentActivity by rememberUpdatedState(activity)
    val motion = remember(target) { PointerMotion() }
    val detents = remember(target) { WheelDetents() }
    var pressed by remember(target) { mutableStateOf(false) }
    var direction by remember(target) { mutableIntStateOf(0) }
    var pulse by remember(target) { mutableIntStateOf(0) }
    val background by animateColorAsState(if (pressed || direction != 0) KiteColors.AccentSurface else KiteColors.Raised, label = "wheel-press")
    fun haptic(effect: Int) { currentFeedback?.invoke(effect) ?: view.performHapticFeedback(effect) }
    fun send(units: Int, tick: Boolean): Boolean {
        if (!enabled || units == 0 || !currentBefore() || !currentScroll(units)) return false
        direction = if (units > 0) 1 else -1
        pulse++
        if (tick && detents.accept(units, SystemClock.uptimeMillis())) haptic(HapticFeedbackConstants.CLOCK_TICK)
        return true
    }
    LaunchedEffect(pressed, pulse) { if (!pressed) { delay(140); direction = 0 } }
    Box(modifier.background(background).semantics {
        contentDescription = label
        stateDescription = if (pressed) pressedLabel else readyLabel
        customActions = listOf(
            CustomAccessibilityAction(up) { if (send(120, false)) { haptic(HapticFeedbackConstants.VIRTUAL_KEY); true } else false },
            CustomAccessibilityAction(down) { if (send(-120, false)) { haptic(HapticFeedbackConstants.VIRTUAL_KEY); true } else false },
        )
    }.pointerInput(target, enabled) {
        if (!enabled) return@pointerInput
        awaitEachGesture {
            val first = awaitFirstDown(requireUnconsumed = false)
            currentActivity()
            first.consume()
            pressed = true
            direction = if (first.position.y < size.height / 2f) 1 else -1
            motion.reset(); detents.reset()
            haptic(HapticFeedbackConstants.VIRTUAL_KEY)
            var pending = Offset.Zero
            var moved = false
            var ended = false
            try {
                do {
                    val event = awaitPointerEvent()
                    currentActivity()
                    val point = event.changes.firstOrNull { it.id == first.id } ?: break
                    if (point.isConsumed) break
                    val released = point.changedToUp()
                    pending += point.position - point.previousPosition
                    if (event.changes.count { it.pressed } > 1) break
                    if (point.pressed && (moved || pending.getDistance() > viewConfiguration.touchSlop)) {
                        val dy = if (moved) point.position.y - point.previousPosition.y else pending.y
                        moved = true
                        send(motion.scroll(dy, density), true)
                    }
                    point.consume()
                    if (!point.pressed) { ended = released; break }
                } while (true)
                if (ended && !moved) send(if (first.position.y < size.height / 2f) 120 else -120, false)
            } finally {
                pressed = false
                pulse++
            }
        }
    }) {
        Canvas(Modifier.fillMaxSize()) {
            val cx = size.width / 2
            val r = 3.5f * density
            drawRoundRect(KiteColors.MouseLine, Offset(cx - 10 * density, size.height * .11f),
                Size(20 * density, size.height * .78f), CornerRadius(6 * density), style = Stroke(density))
            fun chevron(y: Float, upward: Boolean, active: Boolean) {
                val color = if (active) KiteColors.Accent else KiteColors.Muted
                val tip = if (upward) y - r / 2 else y + r / 2
                val side = if (upward) y + r / 2 else y - r / 2
                drawLine(color, Offset(cx - r, side), Offset(cx, tip), 1.5f * density, StrokeCap.Round)
                drawLine(color, Offset(cx, tip), Offset(cx + r, side), 1.5f * density, StrokeCap.Round)
            }
            chevron(size.height * .24f, true, direction > 0)
            chevron(size.height * .76f, false, direction < 0)
            val middle = size.height / 2
            for (i in -1..1) drawLine(if (pressed) KiteColors.Accent else KiteColors.MouseLine,
                Offset(cx - r, middle + i * 3 * density), Offset(cx + r, middle + i * 3 * density), density, StrokeCap.Round)
        }
    }
}
