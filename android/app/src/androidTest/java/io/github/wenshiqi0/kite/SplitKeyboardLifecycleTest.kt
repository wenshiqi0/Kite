package io.github.wenshiqi0.kite

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.os.Build
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.view.inputmethod.InputMethodManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Real workspace, SurfaceHolder callbacks and Android IME; synthetic state, no host. */
class SplitKeyboardLifecycleTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun main(block: () -> Unit) {
        var failure: Throwable? = null
        test.runOnMainSync { try { block() } catch (error: Throwable) { failure = error } }
        failure?.let { throw it }
    }
    private fun waitFor(label: String, condition: () -> Boolean) {
        val end = SystemClock.uptimeMillis() + 8000
        while (!condition() && SystemClock.uptimeMillis() < end) SystemClock.sleep(50)
        assertTrue(label, condition())
    }
    private fun video(view: View): VideoSurface? {
        if (view is VideoSurface) return view
        if (view is ViewGroup) for (i in 0 until view.childCount) video(view.getChildAt(i))?.let { return it }
        return null
    }
    private fun node(description: String): AccessibilityNodeInfo? {
        fun visit(value: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            value ?: return null
            if (value.isVisibleToUser && value.contentDescription?.toString() == description) return value
            for (i in 0 until value.childCount) visit(value.getChild(i))?.let { return it }
            return null
        }
        if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
        return visit(test.uiAutomation.rootInActiveWindow)
    }
    private fun click(description: String) {
        var target = requireNotNull(node(description)) { "Missing control: $description" }
        while (!target.isClickable) target = requireNotNull(target.parent)
        assertTrue(target.isEnabled)
        assertTrue(target.performAction(AccessibilityNodeInfo.ACTION_CLICK))
    }
    private fun imeVisible(view: View) = ViewCompat.getRootWindowInsets(view)?.isVisible(WindowInsetsCompat.Type.ime()) == true
    private fun served(video: VideoSurface) = (video.context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).isActive(video)
    private fun screenshot(name: String) {
        val bitmap = test.uiAutomation.takeScreenshot()
        File(test.targetContext.getExternalFilesDir(null), name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }
    private fun fixture(check: (HostCheckViewModel, VideoSurface, View) -> Unit) {
        assumeTrue(InstrumentationRegistry.getArguments().getString("qa_native_history") == "true")
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        main { vm = ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        waitFor("Core initialized") { vm.engine.state.value?.initialized == true }
        val saved = requireNotNull(vm.engine.state.value)
        assumeTrue("Only an empty isolated app may use this fixture", saved.hosts.isEmpty())
        @Suppress("UNCHECKED_CAST")
        val display = vm.engine.javaClass.getDeclaredField("current").apply { isAccessible = true }.get(vm.engine) as MutableStateFlow<CoreSnapshot?>
        // No selected host: renderer-ready/gone callbacks cannot start a network stream.
        val snapshot = saved.copy(connection = ConnectionStatus.CONNECTED, selectedHost = null,
            desktop = saved.desktop.copy(status = DesktopStatus.STREAMING, takeover = true, sourceWidth = 1920u, sourceHeight = 1080u))
        val root = activity.findViewById<View>(android.R.id.content)
        try {
            main { vm.settingsVisible = false; display.value = snapshot; vm.changeLayout(DesktopLayout.Split, rotate = false) }
            waitFor("Real split workspace has a renderer") { video(root)?.target?.let { it != 0L } == true && node("键盘") != null }
            val surface = requireNotNull(video(root))
            waitFor("Split video can receive input") { surface.takeover && surface.gesturesEnabled && surface.hasWindowFocus() }
            check(vm, surface, root)
        } catch (error: Throwable) {
            runCatching { screenshot("split-keyboard-failure.png") }
            throw error
        } finally {
            main { video(root)?.closeRemoteKeyboard(); display.value = saved; activity.finish() }
        }
    }
    private fun open(video: VideoSurface, label: String) {
        click("键盘")
        waitFor(label) { imeVisible(video) && served(video) && video.remoteKeyboardActive }
        test.waitForIdleSync()
        SystemClock.sleep(350)
    }

    @Test fun toolbarReopensAfterTheSameVideoViewRecreatesItsSurface() = fixture { vm, surface, root ->
        open(surface, "Initial toolbar keyboard opens")
        repeat(3) { cycle ->
            main { surface.closeRemoteKeyboard() }
            waitFor("Keyboard hidden before recreation") { !imeVisible(surface) }
            val oldTarget = surface.target
            // Visibility drives actual SurfaceHolder destruction/recreation without
            // removing its AndroidView from composition or calling its factory again.
            main { surface.getChildAt(0).visibility = View.INVISIBLE }
            waitFor("Old renderer is destroyed") { surface.target == 0L }
            main { surface.getChildAt(0).visibility = View.VISIBLE }
            waitFor("The retained view has a new renderer") { surface.target != 0L && surface.target != oldTarget }
            assertSame("This must exercise Surface recreation rather than a new view factory", surface, video(root))
            open(surface, "Toolbar reopens the remote keyboard after Surface recreation $cycle")
            assertNotNull("The current editor keeps its text flush callback", vm.flushRemoteText)
        }
        screenshot("split-keyboard-after-recreation.png")
    }

    @Test fun splitToolbarSwitchesBetweenChatTouchpadAndRemoteInput() = fixture { vm, surface, _ ->
        repeat(3) {
            click("消息输入框")
            waitFor("Chat owns the local keyboard") { imeVisible(surface) && !surface.hasFocus() }
            open(surface, "Toolbar takes the keyboard back from chat")
            main { surface.closeRemoteKeyboard(); vm.toggleTouchpad() }
            waitFor("Touchpad appears without the keyboard") { vm.touchpadVisible && !imeVisible(surface) }
            open(surface, "Toolbar opens while hiding the touchpad")
            assertFalse(vm.touchpadVisible)
        }
        screenshot("split-keyboard-after-chat-and-touchpad.png")
    }
}
