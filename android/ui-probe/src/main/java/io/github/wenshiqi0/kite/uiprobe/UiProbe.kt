package io.github.wenshiqi0.kite.uiprobe

import android.app.Activity
import android.app.Instrumentation
import android.accessibilityservice.AccessibilityServiceInfo
import android.os.Bundle
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import android.graphics.Rect
import android.view.MotionEvent
import android.view.InputDevice

/** Runs in its own test-only package so it does not restart the streaming app. */
class UiProbe : Instrumentation() {
    private var text: String? = null
    private var options = Bundle()
    override fun onCreate(arguments: Bundle?) { super.onCreate(arguments); options = arguments ?: Bundle(); text = options.getString("text"); start() }
    override fun onStart() {
        try {
            val info = uiAutomation.serviceInfo
            info.flags = info.flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS
            uiAutomation.serviceInfo = info
            val deadline = SystemClock.uptimeMillis() + 3000
            var candidate = uiAutomation.rootInActiveWindow
            while (candidate == null && SystemClock.uptimeMillis() < deadline) { SystemClock.sleep(50); candidate = uiAutomation.rootInActiveWindow }
            val root = checkNotNull(candidate) { "no active root" }
            check(root.packageName?.toString() == "io.github.wenshiqi0.kite") { "wrong root package: ${root.packageName}" }
            val gesture = options.getString("gesture")
            if (gesture != null) {
                val targets = mutableListOf<AccessibilityNodeInfo>()
                fun find(node: AccessibilityNodeInfo) {
                    if (node.contentDescription?.toString() == "远程桌面画面") targets.add(node)
                    for (i in 0 until node.childCount) node.getChild(i)?.let(::find)
                }
                find(root)
                check(targets.size == 1) { "one live desktop viewport required" }
                val bounds = Rect(); targets.single().getBoundsInScreen(bounds)
                injectGesture(gesture, bounds)
                finish(Activity.RESULT_OK, Bundle().apply { putBoolean("gesture_sent", true) })
                return
            }
            val value = checkNotNull(text) { "text or gesture argument required" }
            val nodes = mutableListOf<AccessibilityNodeInfo>()
            fun visit(node: AccessibilityNodeInfo) {
                if (node.className?.toString() == "android.widget.EditText" && node.isEnabled) nodes.add(node)
                for (index in 0 until node.childCount) node.getChild(index)?.let(::visit)
            }
            visit(root)
            check(nodes.size == 1) { "one visible text composer required, found ${nodes.size}" }
            nodes.single().performAction(AccessibilityNodeInfo.ACTION_FOCUS)
            val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value) }
            check(nodes.single().performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) { "set-text rejected" }
            finish(Activity.RESULT_OK, Bundle().apply { putBoolean("text_set", true) })
        } catch (e: Exception) {
            finish(Activity.RESULT_CANCELED, Bundle().apply { putString("failure", "${e.javaClass.simpleName}: ${e.message}") })
        }
    }
    private fun injectGesture(kind: String, bounds: Rect) {
        check(kind in listOf("pinch", "pan", "right_tap", "scroll"))
        val cx = bounds.exactCenterX(); val cy = bounds.exactCenterY()
        val distance = options.getString("distance")?.toFloat() ?: 160f
        val factor = options.getString("factor")?.toFloat() ?: 2f
        val dx = options.getString("dx")?.toFloat() ?: 120f
        val dy = options.getString("dy")?.toFloat() ?: 0f
        val down = SystemClock.uptimeMillis()
        fun send(action: Int, count: Int, progress: Float) {
            val separation = if (kind == "pinch") distance * (1 + (factor - 1) * progress) else distance
            val moveX = if (kind == "pan" || kind == "scroll") dx * progress else 0f
            val moveY = if (kind == "pan" || kind == "scroll") dy * progress else 0f
            val props = Array(count) { i -> MotionEvent.PointerProperties().apply { id = i; toolType = MotionEvent.TOOL_TYPE_FINGER } }
            val coords = Array(count) { i -> MotionEvent.PointerCoords().apply {
                x = cx + moveX + (if (kind == "pan") 0f else if (i == 0) -separation / 2 else separation / 2)
                y = cy + moveY; pressure = 1f; size = .05f
            } }
            val event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, count, props, coords, 0, 0, 1f, 1f, 0, 0, InputDevice.SOURCE_TOUCHSCREEN, 0)
            try { check(uiAutomation.injectInputEvent(event, true)) { "touch injection failed" } } finally { event.recycle() }
        }
        send(MotionEvent.ACTION_DOWN, 1, 0f)
        if (kind != "pan") send(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 2, 0f)
        if (kind != "right_tap") for (i in 1..16) { SystemClock.sleep(16); send(MotionEvent.ACTION_MOVE, if (kind == "pan") 1 else 2, i / 16f) }
        if (kind != "pan") send(MotionEvent.ACTION_POINTER_UP or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 2, 1f)
        send(MotionEvent.ACTION_UP, 1, 1f)
    }

}
