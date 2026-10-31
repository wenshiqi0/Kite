package io.github.wenshiqi0.kite.media

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.view.InputDevice
import android.view.MotionEvent
import android.view.KeyEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.ViewConfiguration
import android.view.View
import android.widget.FrameLayout
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import io.github.wenshiqi0.kite.R
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.roundToInt

/** Stable video plane inside a clipped viewport; gestures use parent coordinates. */
class VideoSurface(context: Context, private val ready: (Long) -> Unit, private val gone: (Long) -> Unit) : FrameLayout(context), SurfaceHolder.Callback {
    private val video = SurfaceView(context)
    private var renderer: VideoRenderer? = null
    private val viewport = DesktopViewport()
    private val desktopPointer = DesktopPointer(viewport)
    private var virtualMouseVisible = false
    private val cursor = object : View(context) {
        private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; style = Paint.Style.FILL }
        private val outline = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF111316.toInt(); style = Paint.Style.STROKE; strokeWidth = 2f; strokeJoin = Paint.Join.ROUND }
        private val arrow = Path().apply {
            moveTo(0f, 0f); lineTo(1f, 23f); lineTo(7f, 18f); lineTo(12f, 28f)
            lineTo(17f, 25f); lineTo(12f, 16f); lineTo(20f, 15f); close()
        }
        init { importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO; visibility = INVISIBLE }
        override fun onDraw(canvas: Canvas) {
            val point = desktopPointer.view ?: return
            canvas.save()
            canvas.translate(point.x, point.y)
            val scale = resources.displayMetrics.density * 20f / 28f
            canvas.scale(scale, scale)
            canvas.drawPath(arrow, fill); canvas.drawPath(arrow, outline)
            canvas.restore()
        }
    }
    val virtualPointerSource: DesktopViewport.Point? get() = desktopPointer.source
    val virtualPointerView: DesktopViewport.Point? get() = desktopPointer.view
    var fullscreenInput = false
        set(value) { field = value; updateInputRoute() }
    private val pointerMotion = PointerMotion()
    var externalMouseButtons = 0
    var onPaste: (() -> Boolean)? = null
    private var pasteKey = false
    private var externalGesture = false
    private var bridgedPointer = false
    private var bridgedX=0f;private var bridgedY=0f
    private var remoteEditor=false
    private var keyboardTarget=0L
    private var connection:RemoteInputConnection?=null
    var onTextFailure:()->Unit={}
    var editorModifiers=0
        set(value){if(field!=value)flushRemoteText();field=value}
    private val inputHandler=Handler(Looper.getMainLooper())
    private val textBuffer=RemoteTextBuffer(
        ::sendCommittedText,
        { task->inputHandler.postDelayed({task()},if(DesktopInput.buffered(target))0 else 200) },
        {onTextFailure()},
    )
    val target: Long get() = renderer?.id ?: 0L
    var takeover = false
        set(value) { if (field != value) { if(!value)closeRemoteKeyboard();cancelGesture(); field = value; updateInputRoute(); refreshCursor() } }
    var trackpad = true
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var panMode = false
        set(value) { if (field != value) { cancelGesture(); field = value } }
    var gesturesEnabled = true
        set(value) { if (field != value) { cancelGesture(); field = value; updateInputRoute(); refreshCursor() } }
    var pip = false
        set(value) { if (field != value) { field = value; updateInputRoute(); applyViewport() } }
    var onZoomChanged: (Float) -> Unit = {}
    var zoom: Float
        get() = viewport.zoom
        set(value) { if (abs(value - viewport.zoom) > .001f) { viewport.zoomAt(value); applyViewport() } }
    fun setSourceSize(w: Int, h: Int) {
        if (w > 0 && h > 0 && (viewport.sourceWidth != w.toFloat() || viewport.sourceHeight != h.toFloat())) {
            viewport.resize(width.toFloat(), height.toFloat(), w.toFloat(), h.toFloat())
            requestLayout()
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
            DesktopInput.mouseButton(target, 1, true); dragging = true
        }
    }
    init {
        clipChildren = true; clipToPadding = true; keepScreenOn = true; isFocusableInTouchMode = true
        contentDescription = context.getString(R.string.desktop_surface)
        video.holder.addCallback(this)
        addView(video, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(cursor, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
    }
    override fun onInterceptTouchEvent(event: MotionEvent) = true
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        setMeasuredDimension(
            resolveSize(MeasureSpec.getSize(widthMeasureSpec), widthMeasureSpec),
            resolveSize(MeasureSpec.getSize(heightMeasureSpec), heightMeasureSpec),
        )
        viewport.resize(measuredWidth.toFloat(), measuredHeight.toFloat())
        // Measure the plane in this pass. A requestLayout from onSizeChanged can
        // otherwise leave a Compose-hosted Surface at its previous (initially 1px) size.
        val w = (viewport.sourceWidth * viewport.fitScale).roundToInt().coerceAtLeast(1)
        val h = (viewport.sourceHeight * viewport.fitScale).roundToInt().coerceAtLeast(1)
        video.measure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY))
        cursor.measure(MeasureSpec.makeMeasureSpec(measuredWidth, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(measuredHeight, MeasureSpec.EXACTLY))
    }
    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val x = (width - video.measuredWidth) / 2
        val y = (height - video.measuredHeight) / 2
        video.layout(x, y, x + video.measuredWidth, y + video.measuredHeight)
        cursor.layout(0, 0, width, height)
        applyViewport()
    }
    private fun applyViewport() {
        if (video.width <= 0 || video.height <= 0) return
        if (virtualMouseVisible && fullscreenInput && takeover && gesturesEnabled && !pip) desktopPointer.keepDesktopUnderHotspot()
        val scale = if (pip) viewport.fitScale else viewport.scale
        val cx = if (pip) viewport.sourceWidth / 2 else viewport.centerX
        val cy = if (pip) viewport.sourceHeight / 2 else viewport.centerY
        video.pivotX = 0f; video.pivotY = 0f
        video.scaleX = viewport.sourceWidth * scale / video.width
        video.scaleY = viewport.sourceHeight * scale / video.height
        video.translationX = viewport.width / 2 - cx * scale - video.left
        video.translationY = viewport.height / 2 - cy * scale - video.top
        refreshCursor()
        if (virtualMouseVisible && takeover && gesturesEnabled && !pip) sendPointer()
        onZoomChanged(viewport.zoom)
    }
    private fun updateInputRoute() {
        val enabled = fullscreenInput && takeover && gesturesEnabled && !pip
        DesktopInput.configure(target, enabled) { onTextFailure() }
        if (viewport.inputGutter(if (enabled) resources.displayMetrics.density * 176 else 0f)) applyViewport()
    }
    private fun refreshCursor() {
        cursor.visibility = if (virtualMouseVisible && takeover && gesturesEnabled && !pip) VISIBLE else INVISIBLE
        cursor.invalidate()
    }
    fun openVirtualMouse(x: Float, y: Float) {
        virtualMouseVisible = true
        desktopPointer.place(x, y, clamp = true)
        applyViewport()
        prepareVirtualMouse()
    }
    fun anchorVirtualMouse(x: Float, y: Float) {
        if (!virtualMouseVisible) return
        desktopPointer.place(x, y, clamp = true)
        applyViewport()
    }
    fun panVirtualMouse(dx: Float, dy: Float) {
        if (!virtualMouseVisible || !takeover || !gesturesEnabled || pip) return
        viewport.pan(dx, dy); applyViewport()
    }
    fun closeVirtualMouse() { virtualMouseVisible = false; refreshCursor() }
    fun virtualMouseActivity() { DesktopInput.activity(target) }
    fun moveVirtualMouse(dx: Float, dy: Float) {
        if (!virtualMouseVisible || !takeover || !gesturesEnabled || pip) return
        desktopPointer.move(dx, dy)
        refreshCursor()
        sendPointer()
    }
    /** Resend the visible hotspot before every button/wheel, never the widget/finger position. */
    fun prepareVirtualMouse(): Boolean {
        if (!virtualMouseVisible || !takeover || !gesturesEnabled || pip || target == 0L) return false
        flushRemoteText()
        refreshCursor()
        return sendPointer()
    }
    private fun sendPointer(): Boolean {
        val p = desktopPointer.source ?: return false
        return target != 0L && DesktopInput.mousePosition(target, p.x.roundToInt(), p.y.roundToInt(), viewport.sourceWidth.toInt(), viewport.sourceHeight.toInt())
    }
    override fun surfaceCreated(holder: SurfaceHolder) = Unit
    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        // Creation precedes the first size callback. Start only when the plane is
        // usable; keep the same renderer through subsequent split/zoom changes.
        if (renderer == null && width > 1 && height > 1 && holder.surface.isValid) {
            renderer = VideoRenderer(holder.surface).also { ready(it.id) }
            updateInputRoute()
        }
    }
    override fun surfaceDestroyed(holder: SurfaceHolder) {
        closeRemoteKeyboard();cancelGesture();closeVirtualMouse();desktopPointer.clear(); renderer?.let { gone(it.id); it.release() }; renderer = null
        DesktopInput.configure(0, false) { onTextFailure() }
    }
    private fun cancelGesture() {
        removeCallbacks(longPress)
        if (target != 0L) DesktopInput.releaseInput(target)
        dragging = false; buttons = 0; multiple = false; moved = false; lastTap = 0L; externalGesture = false; bridgedPointer = false; pasteKey = false; pointerMotion.reset()
    }
    /** A gesture starting on a Compose button has no DOWN in AndroidView.
     * The shared parent routes only the separate canvas finger through here. */
    fun bridgePointer(x: Float, y: Float) {
        if (!takeover || !gesturesEnabled || target == 0L) return
        DesktopInput.activity(target)
        removeCallbacks(longPress)
        val first=!bridgedPointer
        if(first){requestFocus();bridgedX=x;bridgedY=y}
        bridgedPointer = true; externalGesture = true; moved = true
        if(trackpad) {
            val delta=pointerMotion.move(x-bridgedX,y-bridgedY,resources.displayMetrics.density)
            if(delta.x!=0||delta.y!=0)DesktopInput.mouseMove(target,delta.x,delta.y)
        } else position(x, y, true)
        bridgedX=x;bridgedY=y
    }
    fun finishBridgedPointer() { bridgedPointer = false; externalGesture = true; moved = true }
    private fun position(x: Float, y: Float, clamp: Boolean = false): Boolean {
        val p = viewport.sourceAt(x, y, clamp) ?: return false
        return DesktopInput.mousePosition(target, p.x.roundToInt(), p.y.roundToInt(), viewport.sourceWidth.toInt(), viewport.sourceHeight.toInt())
    }
    private fun click(button: Int) { DesktopInput.mouseButton(target, button, true); DesktopInput.mouseButton(target, button, false) }
    private fun focus(event: MotionEvent): DesktopViewport.Point {
        var x = 0f; var y = 0f
        for (index in 0 until event.pointerCount) { x += event.getX(index); y += event.getY(index) }
        return DesktopViewport.Point(x / event.pointerCount, y / event.pointerCount)
    }
    private fun span(event: MotionEvent): Float = if (event.pointerCount > 1) hypot(event.getX(1) - event.getX(0), event.getY(1) - event.getY(0)).coerceAtLeast(1f) else 1f
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (!gesturesEnabled || pip) return true
        DesktopInput.activity(target)
        if(event.actionMasked==MotionEvent.ACTION_DOWN)flushRemoteText()
        if (bridgedPointer) return true
        if (takeover && target != 0L && event.isFromSource(InputDevice.SOURCE_MOUSE)) { mouse(event); return true }
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                multiple = false; moved = false; dragging = false; rightTap = false; multi = Multi.Pending
                downX = event.x; downY = event.y; lastX = event.x; lastY = event.y
                externalGesture = takeover && externalMouseButtons != 0
                if (externalGesture) { position(event.x, event.y, true); return true }
                if (takeover && target != 0L && !panMode) {
                    requestFocus()
                    val doubleTap = trackpad && event.eventTime - lastTap in 1..ViewConfiguration.getDoubleTapTimeout().toLong() && hypot(event.x - tapX, event.y - tapY) < doubleSlop
                    if (doubleTap) { DesktopInput.mouseButton(target, 1, true); dragging = true; lastTap = 0L }
                    else postDelayed(longPress, ViewConfiguration.getLongPressTimeout().toLong())
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                removeCallbacks(longPress)
                if (dragging && target != 0L) DesktopInput.mouseButton(target, 1, false)
                dragging = false; multiple = true; rightTap = event.pointerCount == 2; multi = Multi.Pending
                val f = focus(event); lastX = f.x; lastY = f.y; lastSpan = span(event)
            }
            MotionEvent.ACTION_MOVE -> if (event.pointerCount >= 2) {
                val f = focus(event); val nextSpan = span(event); val dx = f.x - lastX; val dy = f.y - lastY
                val pinch = abs(nextSpan / lastSpan - 1f) > .035f && abs(nextSpan - lastSpan) > slop
                if (event.pointerCount >= 3 || panMode || !takeover || !trackpad) multi = Multi.Pan
                else if (multi == Multi.Pending && pinch) multi = Multi.Zoom
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
                            if (x != 0 || y != 0) { DesktopInput.scroll(target, x, y); wheelX -= x; wheelY -= y }
                        }
                        else -> Unit
                    }
                    lastX = f.x; lastY = f.y; lastSpan = nextSpan
                }
            } else if (!multiple) {
                val dx = event.x - lastX; val dy = event.y - lastY
                if (externalGesture) { position(event.x, event.y, true); moved = true }
                else if (!takeover || panMode || viewport.zoom>1.01f&&!trackpad&&!dragging) {
                    if(!takeover||panMode||moved||hypot(event.x-downX,event.y-downY)>slop) {
                        removeCallbacks(longPress)
                        viewport.pan(if(moved)dx else event.x-downX,if(moved)dy else event.y-downY)
                        applyViewport();moved=true
                    }
                }
                else if (target != 0L && trackpad) {
                    if (hypot(event.x - downX, event.y - downY) > slop || moved || dragging) {
                        removeCallbacks(longPress)
                        val d = pointerMotion.move(if (moved) dx else event.x - downX, if (moved) dy else event.y - downY, resources.displayMetrics.density)
                        moved = true; relativeX += d.x; relativeY += d.y
                        val x = relativeX.toInt(); val y = relativeY.toInt()
                        if (x != 0 || y != 0) { DesktopInput.mouseMove(target, x, y); relativeX -= x; relativeY -= y }
                    }
                } else if (target != 0L) {
                    if (!dragging && hypot(event.x - downX, event.y - downY) > slop) {
                        removeCallbacks(longPress)
                        if (position(downX, downY)) { DesktopInput.mouseButton(target, 1, true); dragging = true }
                    }
                    if (dragging) { position(event.x, event.y, true); moved = true }
                }
                lastX = event.x; lastY = event.y
            }
            MotionEvent.ACTION_POINTER_UP -> {
                if(event.pointerCount==2&&(multi==Multi.Pan||multi==Multi.Zoom)){
                    val remaining=if(event.actionIndex==0)1 else 0
                    lastX=event.getX(remaining);lastY=event.getY(remaining)
                    downX=lastX;downY=lastY;multiple=false;moved=true;rightTap=false
                }
            }
            MotionEvent.ACTION_UP -> {
                removeCallbacks(longPress)
                if (takeover && !panMode && target != 0L && !externalGesture) {
                    if (dragging) DesktopInput.mouseButton(target, 1, false)
                    else if (multiple) { if (rightTap && !moved && event.eventTime - event.downTime < 400 && (trackpad || position(lastX, lastY))) click(3) }
                    else if (!moved && (trackpad || position(event.x, event.y))) {
                        click(1); lastTap = event.eventTime; tapX = event.x; tapY = event.y
                    }
                }
                dragging = false; externalGesture = false; performClick()
            }
            MotionEvent.ACTION_CANCEL -> cancelGesture()
        }
        return true
    }
    private fun mouse(event: MotionEvent) {
        if (!position(event.x, event.y, dragging || buttons != 0)) return
        val next = event.buttonState
        if(next and buttons.inv()!=0)flushRemoteText()
        for ((mask, button) in listOf(MotionEvent.BUTTON_PRIMARY to 1, MotionEvent.BUTTON_SECONDARY to 3, MotionEvent.BUTTON_TERTIARY to 2)) {
            if ((next xor buttons) and mask != 0) DesktopInput.mouseButton(target, button, next and mask != 0)
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
            if (event.actionMasked == MotionEvent.ACTION_SCROLL) DesktopInput.scroll(target, (event.getAxisValue(MotionEvent.AXIS_HSCROLL) * 120).toInt(), (event.getAxisValue(MotionEvent.AXIS_VSCROLL) * 120).toInt())
            return true
        }
        return super.onGenericMotionEvent(event)
    }
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, true) || super.onKeyDown(keyCode, event)
    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean = keyboard(keyCode, event, false) || super.onKeyUp(keyCode, event)
    private fun keyboard(keyCode: Int, event: KeyEvent, down: Boolean): Boolean {
        if (!takeover || !gesturesEnabled || target == 0L) return false
        flushRemoteText()
        if(keyCode==KeyEvent.KEYCODE_V) {
            if(!down && pasteKey){pasteKey=false;return true}
            if(down && pasteKey)return true
            if(down && (event.isCtrlPressed || event.isMetaPressed) && onPaste?.invoke()==true){pasteKey=true;return true}
        }
        val key = virtualKey(keyCode) ?: return false
        val modifiers = (if (event.isShiftPressed) 1 else 0) or (if (event.isCtrlPressed) 2 else 0) or (if (event.isAltPressed) 4 else 0) or (if (event.isMetaPressed) 8 else 0)
        return DesktopInput.key(target, key, down, modifiers)
    }
    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) { super.onFocusChanged(gainFocus, direction, previouslyFocusedRect); if (!gainFocus && target != 0L) {flushRemoteText();DesktopInput.releaseInput(target)} }
    override fun performClick(): Boolean = super.performClick()

    fun flushRemoteText(){textBuffer.flush()}
    fun openRemoteKeyboard():Boolean {
        if(!takeover||!gesturesEnabled||target==0L)return false
        remoteEditor=true;keyboardTarget=target;requestFocus()
        val input=context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        input.restartInput(this)
        post{if(remoteEditor&&keyboardTarget==target)input.showSoftInput(this,InputMethodManager.SHOW_IMPLICIT)}
        return true
    }
    fun closeRemoteKeyboard(hide:Boolean=true) {
        if(remoteEditor){connection?.cancelComposition();textBuffer.flush()}
        remoteEditor=false;keyboardTarget=0;connection=null;textBuffer.cancel()
        if(hide&&windowToken!=null)(context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(windowToken,0)
    }
    override fun onCheckIsTextEditor():Boolean=remoteEditor&&takeover
    override fun onCreateInputConnection(outAttrs:EditorInfo):InputConnection? {
        if(!onCheckIsTextEditor()||target==0L)return null
        outAttrs.inputType=InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        outAttrs.imeOptions=EditorInfo.IME_ACTION_NONE or EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING
        outAttrs.initialSelStart=1;outAttrs.initialSelEnd=1
        return RemoteInputConnection(this,textBuffer,::editorKey){event->
            keyboard(event.keyCode,event,event.action==KeyEvent.ACTION_DOWN)
        }.also{connection=it}
    }
    private fun editorKey(key:Int):Boolean {
        if(keyboardTarget!=target||!remoteEditor||!takeover||target==0L)return false
        val down=DesktopInput.key(target,key,true,editorModifiers)
        val up=DesktopInput.key(target,key,false,editorModifiers)
        return down&&up
    }
    private fun sendCommittedText(text:String):Boolean {
        if(keyboardTarget!=target||!remoteEditor||!takeover||target==0L)return false
        if(editorModifiers==0)return DesktopInput.text(target,text)
        for(c in text) {
            val upper=c.uppercaseChar()
            if(upper in 'A'..'Z'||upper in '0'..'9') {if(!editorKey(upper.code))return false}
            else if(!DesktopInput.text(target,c.toString()))return false
        }
        return true
    }

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
