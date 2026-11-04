package io.github.wenshiqi0.kite

import android.app.Dialog
import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.TextView
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.material3.OutlinedTextField
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Real Android IME/focus on an owned surface; no host, session or provider calls. */
class RemoteKeyboardTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun main(block: () -> Unit) {
        var failure: Throwable? = null
        test.runOnMainSync { try { block() } catch (error: Throwable) { failure = error } }
        failure?.let { throw it }
    }
    private fun waitFor(label: String, condition: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + 5000
        while (!condition() && SystemClock.uptimeMillis() < deadline) SystemClock.sleep(50)
        assertTrue(label, condition())
    }
    private class Fixture(val video: VideoSurface, val question: () -> Unit, val remote: () -> Boolean) {
        fun imeVisible() = ViewCompat.getRootWindowInsets(video)?.isVisible(WindowInsetsCompat.Type.ime()) == true
        fun remoteServed() = (video.context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).isActive(video)
        fun open() {
            val opened = remote()
            assertTrue("Open: target=${video.target}, attached=${video.isAttachedToWindow}, window=${video.hasWindowFocus()}, focused=${video.isFocused}, focusable=${video.isFocusable}", opened)
        }
    }
    private fun fixture(check: (MainActivity, Fixture) -> Unit) {
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        val original = mutableListOf<View>()
        lateinit var panel: ComposeView
        lateinit var video: VideoSurface
        var panelReady = false
        var videoReady = false
        lateinit var question: () -> Unit
        lateinit var remote: () -> Boolean
        try {
            main {
                for (i in 0 until root.childCount) root.getChildAt(i).let { original.add(it); it.visibility = View.INVISIBLE }
                panel = ComposeView(activity).apply { setContent {
                    KiteTheme {
                        val focus = LocalFocusManager.current
                        val keyboard = LocalSoftwareKeyboardController.current
                        val questionFocus = remember { FocusRequester() }
                        var answer by remember { mutableStateOf("") }
                        question = { questionFocus.requestFocus(); keyboard?.show() }
                        remote = { focus.clearFocus(force = true); video.openRemoteKeyboard() }
                        Column(Modifier.fillMaxSize().imePadding()) {
                            AndroidView(factory = { VideoSurface(it, {}, {}).also { surface -> video = surface; videoReady = true; surface.takeover = true } }, modifier = Modifier.weight(1f).fillMaxWidth())
                            OutlinedTextField(answer, { answer = it }, modifier = Modifier.fillMaxWidth().focusRequester(questionFocus))
                        }
                    }
                } }
                panelReady = true
                root.addView(panel, FrameLayout.LayoutParams(-1, -1))
            }
            test.waitForIdleSync()
            waitFor("Owned video surface ready") { videoReady && video.target != 0L && video.hasWindowFocus() }
            check(activity, Fixture(video, { question() }, { remote() }))
        } finally {
            main {
                if (videoReady) video.closeRemoteKeyboard()
                if (panelReady) { root.removeView(panel); panel.disposeComposition() }
                original.forEach { it.visibility = View.VISIBLE }
                activity.finish()
            }
        }
    }

    @Test fun questionRemoteAndBackCanSwitchRepeatedly() = fixture { _, f ->
        repeat(4) {
            main { f.question() }
            waitFor("Question keyboard shown") { f.imeVisible() && !f.video.hasFocus() }
            assertFalse("Local question must release the remote editor", f.video.onCheckIsTextEditor())
            main { f.open() }
            waitFor("Remote keyboard shown and served") { f.imeVisible() && f.remoteServed() && f.video.hasFocus() }
            assertTrue(f.video.onCheckIsTextEditor())
            test.waitForIdleSync()
            SystemClock.sleep(350) // Let the IME show animation finish before sending Back.
            test.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
            waitFor("Back hides the keyboard") { !f.imeVisible() }
            main { f.open() }
            waitFor("Remote keyboard can reopen after Back") { f.imeVisible() && f.remoteServed() }
            main { f.question() }
            waitFor("Question retakes input from the remote editor") { f.imeVisible() && !f.video.hasFocus() }
            assertFalse("Returning to a question must close the remote editor", f.video.onCheckIsTextEditor())
            main { f.open() }
            waitFor("Remote editor retakes input from the question") { f.imeVisible() && f.remoteServed() }
            main { f.video.closeRemoteKeyboard() }
            waitFor("Explicit close hides keyboard") { !f.imeVisible() }
        }
        main { f.open() }
        waitFor("Remote keyboard for capture") { f.imeVisible() && f.remoteServed() }
        File(test.targetContext.getExternalFilesDir(null), "remote-keyboard.png").outputStream().use {
            test.uiAutomation.takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test fun pendingOpenWaitsForWindowFocusAndCanBeCancelled() = fixture { activity, f ->
        lateinit var dialog: Dialog
        var dialogReady = false
        fun showDialog() {
            main {
                dialog = Dialog(activity).apply {
                    setContentView(TextView(activity).apply { text = "Owned focus transition" })
                    window!!.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN)
                    show()
                }
                dialogReady = true
            }
            waitFor("Dialog owns window focus") { !f.video.hasWindowFocus() }
        }
        try {
            showDialog()
            main { assertTrue(f.remote()) }
            test.waitForIdleSync()
            main { dialog.dismiss() }
            waitFor("Pending remote keyboard resumes after window focus") { f.video.hasWindowFocus() && f.imeVisible() && f.remoteServed() }
            main { f.video.closeRemoteKeyboard() }
            waitFor("Keyboard hidden before cancellation") { !f.imeVisible() }
            showDialog()
            main { assertTrue(f.remote()); f.video.closeRemoteKeyboard(false); dialog.dismiss() }
            test.waitForIdleSync()
            SystemClock.sleep(400)
            assertFalse("Cancelled request must stay closed", f.imeVisible())
            assertFalse(f.video.onCheckIsTextEditor())
        } finally {
            main { if (dialogReady) dialog.dismiss() }
        }
    }

    @Test fun failedFocusDoesNotLeaveARemoteEditor() = fixture { _, f ->
        main {
            f.video.isFocusable = false
            assertFalse("Do not report an open keyboard when focus was refused", f.video.openRemoteKeyboard())
            assertFalse(f.video.onCheckIsTextEditor())
            f.video.isFocusableInTouchMode = true
            assertTrue(f.remote())
        }
        waitFor("A failed focus attempt remains retryable") { f.imeVisible() && f.remoteServed() }
    }
}
