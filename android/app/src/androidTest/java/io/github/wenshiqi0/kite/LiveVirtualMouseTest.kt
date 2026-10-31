package io.github.wenshiqi0.kite

import android.accessibilityservice.AccessibilityServiceInfo
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.SystemClock
import android.os.Build
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import kotlin.math.min

/** Opt-in UI/JNI regression. Coordinates are compared with the owned host's event log. */
class LiveVirtualMouseTest {
    @Test fun mouseButtonsWheelAndDraggingUseTheVisibleArrowHotspot() {
        val args = InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_live") == "true")
        val session = requireNotNull(args.getString("qa_session"))
        require(session.startsWith("kite-qa-history-"))
        val test = InstrumentationRegistry.getInstrumentation()
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        test.runOnMainSync { vm = ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        fun waitFor(condition: () -> Boolean) {
            val deadline = SystemClock.uptimeMillis() + 30000
            while (!condition() && SystemClock.uptimeMillis() < deadline) SystemClock.sleep(100)
            assertTrue("Isolated mouse fixture did not become ready", condition())
        }
        waitFor { vm.engine.state.value?.connection == ConnectionStatus.CONNECTED }
        val state = requireNotNull(vm.engine.state.value)
        val host = state.hosts.single { it.id == state.selectedHost }
        assertEquals("10.0.2.2", host.host); assertEquals(22223, host.port.toInt()); assertEquals(48200, host.sunshinePort.toInt())
        test.runOnMainSync { vm.openSession(session) }
        waitFor { vm.engine.state.value?.selectedSession == session && !vm.sessionBusy }
        test.runOnMainSync { vm.openDesktop(); vm.changeLayout(DesktopLayout.Fullscreen) }
        waitFor { vm.engine.state.value?.desktop?.status == DesktopStatus.STREAMING }
        test.runOnMainSync { vm.takeOver(true) }
        waitFor { vm.engine.state.value?.desktop?.takeover == true }
        assertEquals(1920, vm.engine.state.value!!.desktop.sourceWidth.toInt())
        assertEquals(1080, vm.engine.state.value!!.desktop.sourceHeight.toInt())
        test.uiAutomation.serviceInfo = test.uiAutomation.serviceInfo.apply {
            flags = flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        }
        fun find(view: View): VideoSurface? {
            if (view is VideoSurface) return view
            if (view is ViewGroup) for (i in 0 until view.childCount) find(view.getChildAt(i))?.let { return it }
            return null
        }
        lateinit var video: VideoSurface
        test.runOnMainSync { video = requireNotNull(find(activity.window.decorView)) }
        fun node(description: String): Rect {
            fun search(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
                node ?: return null
                if (node.contentDescription?.toString() == description) return node
                for (i in 0 until node.childCount) search(node.getChild(i))?.let { return it }
                return null
            }
            var found: AccessibilityNodeInfo? = null
            waitFor {
                // The control moves during a gesture. Never tap its cached old bounds.
                if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
                found = search(test.uiAutomation.rootInActiveWindow); found != null
            }
            return Rect().also { found!!.getBoundsInScreen(it) }
        }
        fun touch(rect: Rect, dx: Float = 0f, dy: Float = 0f) {
            val down = SystemClock.uptimeMillis()
            fun event(action: Int, fraction: Float) {
                val event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, rect.exactCenterX() + dx * fraction, rect.exactCenterY() + dy * fraction, 0)
                event.source = InputDevice.SOURCE_TOUCHSCREEN
                try { assertTrue(test.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
            }
            event(MotionEvent.ACTION_DOWN, 0f)
            if (dx != 0f || dy != 0f) for (i in 1..10) { SystemClock.sleep(20); event(MotionEvent.ACTION_MOVE, i / 10f) }
            event(MotionEvent.ACTION_UP, 1f)
            SystemClock.sleep(350)
        }
        if (video.getChildAt(1).visibility != View.VISIBLE) touch(node(activity.getString(R.string.expand_virtual_mouse)))
        waitFor { video.getChildAt(1).visibility == View.VISIBLE }
        val records = JSONArray()
        val output = test.targetContext.getExternalFilesDir(null)
        fun screenshot(name: String) {
            val bitmap = test.uiAutomation.takeScreenshot()
            File(output, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
        }
        fun record(name: String) {
            test.runOnMainSync {
                val p = requireNotNull(video.virtualPointerSource)
                records.put(JSONObject().put("action", name).put("sourceX", p.x.toDouble()).put("sourceY", p.y.toDouble()).put("zoom", vm.zoom.toDouble()))
            }
        }
        record("left-down"); touch(node(activity.getString(R.string.mouse_left)))
        record("right-down"); touch(node(activity.getString(R.string.mouse_right)))
        val wheelBounds = node(activity.getString(R.string.mouse_wheel))
        record("wheel-tap-up"); touch(Rect(wheelBounds.left, wheelBounds.top, wheelBounds.right, wheelBounds.centerY()))
        record("wheel-tap-down"); touch(Rect(wheelBounds.left, wheelBounds.centerY(), wheelBounds.right, wheelBounds.bottom))
        record("wheel"); touch(node(activity.getString(R.string.mouse_wheel)), dy = -140f)
        screenshot("virtual-mouse-expanded.png")
        val before = video.virtualPointerSource!!
        touch(node(activity.getString(R.string.move_virtual_mouse)), dx = -140f, dy = -40f)
        assertTrue(video.virtualPointerSource != before)
        record("moved-left-down"); touch(node(activity.getString(R.string.mouse_left)))
        val dragStart = video.virtualPointerSource!!
        record("drag-down"); touch(node(activity.getString(R.string.mouse_left)), dx = -100f, dy = -40f); record("drag-up")
        assertTrue("Dragging the actual button must move the hotspot", video.virtualPointerSource != dragStart)
        val scale = min(video.width / 1920f, video.height / 1080f) * vm.zoom
        assertEquals("Moving the control must not subtract from the finger delta", dragStart.x - 100f / scale, video.virtualPointerSource!!.x, 3f)
        assertEquals(dragStart.y - 40f / scale, video.virtualPointerSource!!.y, 3f)
        test.runOnMainSync { vm.zoom = 2f }
        SystemClock.sleep(300)
        touch(Rect(vm.videoBounds.centerX() - 10, 150, vm.videoBounds.centerX() + 10, 170), dx = 90f, dy = 40f)
        record("zoom-pan-left-down"); touch(node(activity.getString(R.string.mouse_left)))
        assertEquals(0, vm.heldMouseButtons)
        assertTrue(test.uiAutomation.rootInActiveWindow.findAccessibilityNodeInfosByText(activity.getString(R.string.mouse_move_area)).isEmpty())
        val density = activity.resources.displayMetrics.density
        val mouseWidth = node(activity.getString(R.string.move_virtual_mouse)).width() / density
        val mouseHeight = node(activity.getString(R.string.virtual_mouse)).height() / density
        assertEquals(104f, mouseWidth, 1f); assertEquals(112f, mouseHeight, 1f)
        File(output, "virtual-mouse-validation.json").writeText(JSONObject().put("sourceWidth", 1920).put("sourceHeight", 1080)
            .put("mouseBodyWidthDp", mouseWidth.toDouble()).put("mouseHeightDp", mouseHeight.toDouble()).put("records", records).toString(2))
        screenshot("virtual-mouse-validation.png")
        touch(node(activity.getString(R.string.collapse_virtual_mouse)))
        assertEquals(View.INVISIBLE, video.getChildAt(1).visibility)
        assertEquals(0, vm.heldMouseButtons)
    }
}
