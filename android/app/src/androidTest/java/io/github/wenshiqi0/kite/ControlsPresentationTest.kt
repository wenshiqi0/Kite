package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.SystemClock
import android.view.HapticFeedbackConstants
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.ui.HostConnectionCard
import io.github.wenshiqi0.kite.ui.WheelControl
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Actual Compose input, press state and card affordances; callbacks never contact a host. */
class ControlsPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun onMain(block: () -> Unit) {
        var failure: Throwable? = null
        test.runOnMainSync { try { block() } catch (e: Throwable) { failure = e } }
        failure?.let { throw it }
    }
    private fun withContent(content: @Composable () -> Unit, check: () -> Unit) {
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        lateinit var overlay: ComposeView
        var added = false
        val originals = mutableListOf<Pair<View, Int>>()
        try {
            onMain {
                for (i in 0 until root.childCount) {
                    val view = root.getChildAt(i); originals.add(view to view.importantForAccessibility)
                    view.visibility = View.INVISIBLE; view.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
                }
                overlay = ComposeView(activity).apply { setContent { KiteTheme { content() } } }
                root.addView(overlay, FrameLayout.LayoutParams(-1, -1))
                added = true
            }
            test.waitForIdleSync(); SystemClock.sleep(200)
            check()
        } finally {
            onMain {
                if (added) { overlay.disposeComposition(); root.removeView(overlay) }
                for ((view, accessibility) in originals) { view.visibility = View.VISIBLE; view.importantForAccessibility = accessibility }
                activity.finish()
            }
        }
    }
    private fun node(description: String): AccessibilityNodeInfo {
        fun find(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            node ?: return null
            if (node.contentDescription?.toString() == description) return node
            for (i in 0 until node.childCount) find(node.getChild(i))?.let { return it }
            return null
        }
        val deadline = SystemClock.uptimeMillis() + 5000
        do {
            if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
            find(test.uiAutomation.rootInActiveWindow)?.let { return it }
            SystemClock.sleep(50)
        } while (SystemClock.uptimeMillis() < deadline)
        error("Missing control: $description")
    }
    private fun bounds(description: String) = Rect().also { node(description).getBoundsInScreen(it) }
    private fun event(down: Long, action: Int, x: Float, y: Float) {
        val e = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, x, y, 0)
        e.source = InputDevice.SOURCE_TOUCHSCREEN
        try { assertTrue(test.uiAutomation.injectInputEvent(e, true)) } finally { e.recycle() }
    }
    private fun tap(rect: Rect) {
        val t = SystemClock.uptimeMillis()
        event(t, MotionEvent.ACTION_DOWN, rect.exactCenterX(), rect.exactCenterY())
        event(t, MotionEvent.ACTION_UP, rect.exactCenterX(), rect.exactCenterY())
        test.waitForIdleSync(); SystemClock.sleep(80)
    }
    private fun screenshot(name: String) {
        val image = test.uiAutomation.takeScreenshot()
        File(test.targetContext.getExternalFilesDir(null), name).outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
        image.recycle()
    }

    @Test fun wheelTapsScrollBothDirectionsAndDraggingHasPressAndDetentFeedback() {
        val scroll = mutableListOf<Int>(); val feedback = mutableListOf<Int>()
        withContent({
            Box(Modifier.fillMaxSize().background(KiteColors.Background).windowInsetsPadding(WindowInsets.safeDrawing).padding(top = 100.dp, start = 80.dp)) {
                WheelControl(1, true, { scroll.add(it); true }, Modifier.size(48.dp, 80.dp), feedback = { feedback.add(it) })
            }
        }) {
            val label = test.targetContext.getString(R.string.mouse_wheel)
            val b = bounds(label); val x = b.exactCenterX(); val y = b.top + b.height() / 4f
            var t = SystemClock.uptimeMillis()
            event(t, MotionEvent.ACTION_DOWN, x, y); SystemClock.sleep(180)
            assertEquals(test.targetContext.getString(R.string.wheel_pressed), node(label).stateDescription.toString())
            onMain { assertEquals(listOf(HapticFeedbackConstants.VIRTUAL_KEY), feedback); assertTrue(scroll.isEmpty()) }
            screenshot("wheel-pressed.png")
            event(t, MotionEvent.ACTION_UP, x, y); test.waitForIdleSync()
            onMain { assertEquals(listOf(120), scroll) }
            t = SystemClock.uptimeMillis()
            event(t, MotionEvent.ACTION_DOWN, x, b.top + b.height() * .75f)
            event(t, MotionEvent.ACTION_UP, x, b.top + b.height() * .75f); test.waitForIdleSync()
            onMain { assertEquals(listOf(120, -120), scroll) }
            t = SystemClock.uptimeMillis()
            event(t, MotionEvent.ACTION_DOWN, x, b.exactCenterY())
            for (i in 1..12) { SystemClock.sleep(20); event(t, MotionEvent.ACTION_MOVE, x, b.exactCenterY() - i * 14f) }
            event(t, MotionEvent.ACTION_UP, x, b.exactCenterY() - 168f); test.waitForIdleSync()
            val density = test.targetContext.resources.displayMetrics.density
            onMain {
                assertEquals((168 / density * 120 / 36).toInt().toFloat(), scroll.sum().toFloat(), 1f)
                assertTrue(feedback.contains(HapticFeedbackConstants.CLOCK_TICK))
            }
            val count = scroll.size
            t = SystemClock.uptimeMillis(); event(t, MotionEvent.ACTION_DOWN, x, y)
            event(t, MotionEvent.ACTION_CANCEL, x, y); test.waitForIdleSync()
            onMain { assertEquals("Cancellation must not synthesize a tap", count, scroll.size) }
        }
    }

    @Test fun hostCardsExposeConnectionStateAndKeepConnectionAndEditingSeparate() {
        val activated = mutableListOf<String>(); val edited = mutableListOf<String>()
        val rows = listOf("studio" to ConnectionStatus.CONNECTED, "build-box" to ConnectionStatus.CONNECTING,
            "laptop" to ConnectionStatus.AWAITING_TRUST, "desk" to ConnectionStatus.FAILED, "new-host" to ConnectionStatus.DISCONNECTED)
        withContent({
            Column(Modifier.fillMaxSize().background(KiteColors.Background).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("主机")
                for ((name, status) in rows) {
                    val host = HostRecord(name, name, 2229u, "__hidden_account__", "/private/herdr", "secret.stream.lan", 48099u)
                    HostConnectionCard(host, status, true, { activated.add(name) }, { edited.add(name) })
                }
            }
        }) {
            fun text(node: AccessibilityNodeInfo?): String {
                node ?: return ""
                return node.text.toString() + " " + (0 until node.childCount).joinToString(" ") { text(node.getChild(it)) }
            }
            bounds("打开主机 studio")
            val visible = text(test.uiAutomation.rootInActiveWindow)
            for (label in listOf("已连接", "连接中…", "需要确认", "连接失败", "未连接")) assertTrue(label, visible.contains(label))
            for (detail in listOf("__hidden_account__", "2229", "/private/herdr", "secret.stream.lan", "48099", "SSH")) assertFalse(detail, visible.contains(detail))
            screenshot("host-cards-validation.png")
            tap(bounds("编辑主机 new-host"))
            onMain { assertEquals(listOf("new-host"), edited); assertTrue(activated.isEmpty()) }
            val card = bounds("打开主机 new-host")
            tap(Rect(card.left + 20, card.top + 10, card.left + 60, card.bottom - 10))
            onMain { assertEquals(listOf("new-host"), activated) }
            tap(bounds("打开主机 build-box"))
            onMain { assertEquals("Connecting cards cannot start another operation", listOf("new-host"), activated) }
        }
    }
}
