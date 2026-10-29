package io.github.wenshiqi0.kite.media

import android.content.Context
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import io.github.wenshiqi0.kite.R
import kotlin.math.abs

class VideoSurface(context: Context, private val ready: (Long) -> Unit, private val gone: (Long) -> Unit) : SurfaceView(context), SurfaceHolder.Callback {
    private var renderer: VideoRenderer? = null
    val target: Long get() = renderer?.id ?: 0L
    var takeover: Boolean = false
        set(value) { field = value; if (!value && target != 0L) NativeMedia.releaseInput(target) }
    private var downX = 0f
    private var downY = 0f
    private var dragging = false
    private var multi = false
    private var scrolled = false
    private var lastX = 0f
    private var lastY = 0f
    private var buttons = 0
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private val longPress = Runnable { if (takeover && !multi && target != 0L) { NativeMedia.mouseButton(target, 1, true); dragging = true } }

    init { holder.addCallback(this); keepScreenOn = true; isFocusableInTouchMode = true; contentDescription = context.getString(R.string.desktop_surface) }
    override fun surfaceCreated(holder: SurfaceHolder) { renderer = VideoRenderer(holder.surface).also { ready(it.id) } }
    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit
    override fun surfaceDestroyed(holder: SurfaceHolder) {
        removeCallbacks(longPress)
        renderer?.let { gone(it.id); it.release() }
        renderer = null
    }
    private fun position(x: Float, y: Float) { NativeMedia.mousePosition(target, x.toInt(), y.toInt(), width, height) }
    private fun click(button: Int) { NativeMedia.mouseButton(target, button, true); NativeMedia.mouseButton(target, button, false) }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (!takeover || target == 0L) return true
        requestFocus()
        if (event.isFromSource(InputDevice.SOURCE_MOUSE)) { mouse(event); return true }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                multi = false; dragging = false; scrolled = false; downX = event.x; downY = event.y
                position(event.x, event.y); postDelayed(longPress, ViewConfiguration.getLongPressTimeout().toLong())
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                removeCallbacks(longPress)
                if (dragging) NativeMedia.mouseButton(target, 1, false)
                dragging = false; multi = true
                lastX = (event.getX(0) + event.getX(1)) / 2; lastY = (event.getY(0) + event.getY(1)) / 2
                position(lastX, lastY)
            }
            MotionEvent.ACTION_MOVE -> if (multi && event.pointerCount >= 2) {
                val x = (event.getX(0) + event.getX(1)) / 2; val y = (event.getY(0) + event.getY(1)) / 2
                if (abs(y - lastY) + abs(x - lastX) > 1f) {
                    scrolled = true
                    val scale = 120f / (48f * resources.displayMetrics.density)
                    NativeMedia.scroll(target, ((x - lastX) * scale).toInt(), ((y - lastY) * scale).toInt())
                }
                lastX = x; lastY = y
            } else if (!multi) {
                if (!dragging && abs(event.x - downX) + abs(event.y - downY) > slop) {
                    removeCallbacks(longPress); NativeMedia.mouseButton(target, 1, true); dragging = true
                }
                position(event.x, event.y)
            }
            MotionEvent.ACTION_POINTER_UP -> if (multi && !scrolled) { click(3); scrolled = true }
            MotionEvent.ACTION_UP -> {
                removeCallbacks(longPress)
                if (dragging) NativeMedia.mouseButton(target, 1, false) else if (!multi) { position(event.x, event.y); click(1) }
                dragging = false; performClick()
            }
            MotionEvent.ACTION_CANCEL -> { removeCallbacks(longPress); NativeMedia.releaseInput(target); dragging = false }
        }
        return true
    }

    private fun mouse(event: MotionEvent) {
        position(event.x, event.y)
        val next = event.buttonState
        for ((mask, button) in listOf(MotionEvent.BUTTON_PRIMARY to 1, MotionEvent.BUTTON_SECONDARY to 3, MotionEvent.BUTTON_TERTIARY to 2)) {
            if ((next xor buttons) and mask != 0) NativeMedia.mouseButton(target, button, next and mask != 0)
        }
        buttons = next
    }
    override fun onHoverEvent(event: MotionEvent): Boolean { if (takeover && target != 0L) { position(event.x, event.y); return true }; return super.onHoverEvent(event) }
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (takeover && target != 0L && event.isFromSource(InputDevice.SOURCE_MOUSE)) {
            mouse(event)
            if (event.actionMasked == MotionEvent.ACTION_SCROLL) NativeMedia.scroll(target, (event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 120).toInt(), (event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 120).toInt())
            return true
        }
        return super.onGenericMotionEvent(event)
    }
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, true) || super.onKeyDown(keyCode, event)
    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, false) || super.onKeyUp(keyCode, event)
    private fun keyboard(keyCode: Int, event: KeyEvent, down: Boolean): Boolean {
        if (!takeover || target == 0L) return false
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
