package io.github.wenshiqi0.kite.media

import android.content.Context
import android.view.InputDevice
import android.view.MotionEvent
import android.view.KeyEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import android.view.Gravity
import android.widget.FrameLayout
import io.github.wenshiqi0.kite.R
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt

/** Stable video plane inside a clipped viewport; gestures use parent coordinates. */
class VideoSurface(context: Context, private val ready: (Long) -> Unit, private val gone: (Long) -> Unit) : FrameLayout(context), SurfaceHolder.Callback {
    private val video = SurfaceView(context)
    private var renderer: VideoRenderer? = null
    private val viewport = DesktopViewport()
    val target: Long get() = renderer?.id ?: 0L
    var takeover = false
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var trackpad = true
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var panMode = false
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var gesturesEnabled = true
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var pip = false
        set(value) { if (field != value) { field = value; applyViewport() } }
    var onZoomChanged: (Float) -> Unit = {}
    var zoom: Float
        get() = viewport.zoom
        set(value) { if (abs(value - viewport.zoom) > .001f) { viewport.zoomAt(value); applyViewport() } }
    fun setSourceSize(w: Int, h: Int) {
        if (w > 0 && h > 0 && (viewport.sourceWidth != w.toFloat() || viewport.sourceHeight != h.toFloat())) {
            viewport.resize(width.toFloat(), height.toFloat(), w.toFloat(), h.toFloat()); applyViewport()
        }
    }
    private enum class Multi { Pending, Zoom, Scroll, Pan }
    private var multi = Multi.Pending
    private var multiple = false
    private var rightTap = false
    private var moved = false
    private var dragging = false
    private var downX = 0f; private var downY = 0f
    private var lastX = 0f; private var lastY = 0f; private var lastSpan = 1f
    private var relativeX = 0f; private var relativeY = 0f
    private var wheelX = 0f; private var wheelY = 0f
    private var buttons = 0
    private var lastTap = 0L; private var tapX = 0f; private var tapY = 0f
    private val slop = ViewConfiguration.get(context).scaledTouchSlop.toFloat()
    private val doubleSlop = ViewConfiguration.get(context).scaledDoubleTapSlop.toFloat()
    private val longPress = Runnable {
        if (takeover && gesturesEnabled && !multiple && !panMode && target != 0L && (trackpad || position(downX, downY))) {
            NativeMedia.mouseButton(target, 1, true); dragging = true
        }
    }
    init {
        clipChildren = true; clipToPadding = true; keepScreenOn = true; isFocusableInTouchMode = true
        contentDescription = context.getString(R.string.desktop_surface)
        video.holder.addCallback(this)
        addView(video, LayoutParams(1, 1, Gravity.CENTER))
    }
    override fun onInterceptTouchEvent(event: MotionEvent) = true
    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        viewport.resize(w.toFloat(), h.toFloat()); applyViewport()
    }
    private fun applyViewport() {
        val baseW = (viewport.sourceWidth * viewport.fitScale).roundToInt().coerceAtLeast(1)
        val baseH = (viewport.sourceHeight * viewport.fitScale).roundToInt().coerceAtLeast(1)
        val params = video.layoutParams as LayoutParams
        if (params.width != baseW || params.height != baseH) { params.width = baseW; params.height = baseH; video.layoutParams = params }
        val scale = if (pip) viewport.fitScale else viewport.scale
        video.pivotX = baseW / 2f; video.pivotY = baseH / 2f
        video.scaleX = viewport.sourceWidth * scale / baseW
        video.scaleY = viewport.sourceHeight * scale / baseH
        video.translationX = if (pip) 0f else (viewport.sourceWidth / 2 - viewport.centerX) * scale
        video.translationY = if (pip) 0f else (viewport.sourceHeight / 2 - viewport.centerY) * scale
        onZoomChanged(viewport.zoom)
    }
    override fun surfaceCreated(holder: SurfaceHolder) { renderer = VideoRenderer(holder.surface).also { ready(it.id) } }
    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
    override fun surfaceDestroyed(holder: SurfaceHolder) {
        cancelGesture(); renderer?.let { gone(it.id); it.release() }; renderer = null
    }
    private fun cancelGesture() {
        removeCallbacks(longPress)
        if (target != 0L) NativeMedia.releaseInput(target)
        dragging = false; buttons = 0; multiple = false; moved = false; lastTap = 0L
    }
    private fun position(x: Float, y: Float, clamp: Boolean = false): Boolean {
        val p = viewport.sourceAt(x, y, clamp) ?: return false
        return NativeMedia.mousePosition(target, p.x.roundToInt(), p.y.roundToInt(), viewport.sourceWidth.toInt(), viewport.sourceHeight.toInt())
    }
    private fun click(button: Int) { NativeMedia.mouseButton(target, button, true); NativeMedia.mouseButton(target, button, false) }
    private fun focus(event: MotionEvent): DesktopViewport.Point {
        var x = 0f; var y = 0f
        for (index in 0 until event.pointerCount) { x += event.getX(index); y += event.getY(index) }
        return DesktopViewport.Point(x / event.pointerCount, y / event.pointerCount)
    }
    private fun span(event: MotionEvent): Float = if (event.pointerCount > 1) hypot(event.getX(1) - event.getX(0), event.getY(1) - event.getY(0)).coerceAtLeast(1f) else 1f
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (!gesturesEnabled || pip) return true
        if (takeover && target != 0L && event.isFromSource(InputDevice.SOURCE_MOUSE)) { mouse(event); return true }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                multiple = false; moved = false; dragging = false; rightTap = false; multi = Multi.Pending
                downX = event.x; downY = event.y; lastX = event.x; lastY = event.y
                if (takeover && target != 0L && !panMode) {
                    requestFocus()
                    val doubleTap = trackpad && event.eventTime - lastTap in 1..ViewConfiguration.getDoubleTapTimeout().toLong() && hypot(event.x - tapX, event.y - tapY) < doubleSlop
                    if (doubleTap) { NativeMedia.mouseButton(target, 1, true); dragging = true; lastTap = 0L }
                    else postDelayed(longPress, ViewConfiguration.getLongPressTimeout().toLong())
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                removeCallbacks(longPress)
                if (dragging && target != 0L) NativeMedia.mouseButton(target, 1, false)
                dragging = false; multiple = true; rightTap = event.pointerCount == 2; multi = Multi.Pending
                val f = focus(event); lastX = f.x; lastY = f.y; lastSpan = span(event)
            }
            MotionEvent.ACTION_MOVE -> if (event.pointerCount >= 2) {
                val f = focus(event); val nextSpan = span(event); val dx = f.x - lastX; val dy = f.y - lastY
                val pinch = abs(nextSpan / lastSpan - 1f) > .035f && abs(nextSpan - lastSpan) > slop
                if (event.pointerCount >= 3 || panMode || !takeover || !trackpad) multi = Multi.Pan
                else if (pinch) multi = Multi.Zoom
                else if (multi == Multi.Pending && hypot(dx, dy) > slop * 1.5f) multi = Multi.Scroll
                if (multi != Multi.Pending) {
                    rightTap = false; moved = true
                    when (multi) {
                        Multi.Zoom, Multi.Pan -> {
                            viewport.zoomAt(viewport.zoom * nextSpan / lastSpan, lastX, lastY)
                            viewport.pan(dx, dy); applyViewport()
                        }
                        Multi.Scroll -> if (target != 0L) {
                            wheelX += dx * 120 / (48 * resources.displayMetrics.density)
                            wheelY += dy * 120 / (48 * resources.displayMetrics.density)
                            val x = wheelX.toInt(); val y = wheelY.toInt()
                            if (x != 0 || y != 0) { NativeMedia.scroll(target, x, y); wheelX -= x; wheelY -= y }
                        }
                        else -> Unit
                    }
                    lastX = f.x; lastY = f.y; lastSpan = nextSpan
                }
            } else if (!multiple) {
                val dx = event.x - lastX; val dy = event.y - lastY
                if (!takeover || panMode) { viewport.pan(dx, dy); applyViewport(); moved = true }
                else if (target != 0L && trackpad) {
                    if (hypot(event.x - downX, event.y - downY) > slop || moved || dragging) {
                        removeCallbacks(longPress)
                        val d = viewport.sourceDelta(if (moved) dx else event.x - downX, if (moved) dy else event.y - downY)
                        moved = true; relativeX += d.x; relativeY += d.y
                        val x = relativeX.toInt(); val y = relativeY.toInt()
                        if (x != 0 || y != 0) { NativeMedia.mouseMove(target, x, y); relativeX -= x; relativeY -= y }
                    }
                } else if (target != 0L) {
                    if (!dragging && hypot(event.x - downX, event.y - downY) > slop) {
                        removeCallbacks(longPress)
                        if (position(downX, downY)) { NativeMedia.mouseButton(target, 1, true); dragging = true }
                    }
                    if (dragging) { position(event.x, event.y, true); moved = true }
                }
                lastX = event.x; lastY = event.y
            }
            MotionEvent.ACTION_POINTER_UP -> Unit
            MotionEvent.ACTION_UP -> {
                removeCallbacks(longPress)
                if (takeover && !panMode && target != 0L) {
                    if (dragging) NativeMedia.mouseButton(target, 1, false)
                    else if (multiple) { if (rightTap && !moved && event.eventTime - event.downTime < 400 && (trackpad || position(lastX, lastY))) click(3) }
                    else if (!moved && (trackpad || position(event.x, event.y))) {
                        click(1); lastTap = event.eventTime; tapX = event.x; tapY = event.y
                    }
                }
                dragging = false; performClick()
            }
            MotionEvent.ACTION_CANCEL -> cancelGesture()
        }
        return true
    }
    private fun mouse(event: MotionEvent) {
        if (!position(event.x, event.y, dragging || buttons != 0)) return
        val next = event.buttonState
        for ((mask, button) in listOf(MotionEvent.BUTTON_PRIMARY to 1, MotionEvent.BUTTON_SECONDARY to 3, MotionEvent.BUTTON_TERTIARY to 2)) {
            if ((next xor buttons) and mask != 0) NativeMedia.mouseButton(target, button, next and mask != 0)
        }
        buttons = next
    }
    override fun onHoverEvent(event: MotionEvent): Boolean {
        if (takeover && gesturesEnabled && target != 0L) { position(event.x, event.y); return true }
        return super.onHoverEvent(event)
    }
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (takeover && gesturesEnabled && target != 0L && event.isFromSource(InputDevice.SOURCE_MOUSE)) {
            mouse(event)
            if (event.actionMasked == MotionEvent.ACTION_SCROLL) NativeMedia.scroll(target, (event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 120).toInt(), (event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 120).toInt())
            return true
        }
        return super.onGenericMotionEvent(event)
    }
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, true) || super.onKeyDown(keyCode, event)
    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, false) || super.onKeyUp(keyCode, event)
    private fun keyboard(keyCode: Int, event: KeyEvent, down: Boolean): Boolean {
        if (!takeover || !gesturesEnabled || target == 0L) return false
        val key = virtualKey(keyCode) ?: return false
        val modifiers = (if (event.isShiftPressed) 1 else 0) or (if (event.isCtrlPressed) 2 else 0) or (if (event.isAltPressed) 4 else 0) or (if (event.isMetaPressed) 8 else 0)
        return NativeMedia.key(target, key, down, modifiers)
    }
    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) { super.onFocusChanged(gainFocus, direction, previouslyFocusedRect); if (!gainFocus && target != 0L) NativeMedia.releaseInput(target) }
    override fun performClick(): Boolean = super.performClick()

    private fun virtualKey(key: Int): Int? = when (key) {
        in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> 65 + key - KeyEvent.KEYCODE_A
        in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> 48 + key - KeyEvent.KEYCODE_0
        in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> 112 + key - KeyEvent.KEYCODE_F1
        KeyEvent.KEYCODE_SPACE -> 32; KeyEvent.KEYCODE_ENTER -> 13; KeyEvent.KEYCODE_DEL -> 8; KeyEvent.KEYCODE_TAB -> 9; KeyEvent.KEYCODE_ESCAPE -> 27
        KeyEvent.KEYCODE_DPAD_LEFT -> 37; KeyEvent.KEYCODE_DPAD_UP -> 38; KeyEvent.KEYCODE_DPAD_RIGHT -> 39; KeyEvent.KEYCODE_DPAD_DOWN -> 40
        KeyEvent.KEYCODE_MOVE_HOME -> 36; KeyEvent.KEYCODE_MOVE_END -> 35; KeyEvent.KEYCODE_PAGE_UP -> 33; KeyEvent.KEYCODE_PAGE_DOWN -> 34
        KeyEvent.KEYCODE_FORWARD_DEL -> 46; KeyEvent.KEYCODE_INSERT -> 45
        KeyEvent.KEYCODE_CTRL_LEFT -> 162; KeyEvent.KEYCODE_CTRL_RIGHT -> 163; KeyEvent.KEYCODE_SHIFT_LEFT -> 160; KeyEvent.KEYCODE_SHIFT_RIGHT -> 161
        KeyEvent.KEYCODE_ALT_LEFT -> 164; KeyEvent.KEYCODE_ALT_RIGHT -> 165; KeyEvent.KEYCODE_META_LEFT -> 91; KeyEvent.KEYCODE_META_RIGHT -> 92
        else -> null
    }
}
