package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.os.Build
import android.os.SystemClock
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.layout.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.ui.*
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.File
import java.security.MessageDigest

/** Owned synthetic PNG and native overlays; no host or model call. */
class SessionImagesPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun sha(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
    private fun find(text: String? = null, description: String? = null): AccessibilityNodeInfo? {
        fun walk(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            node ?: return null
            if (text != null && node.text?.toString() == text || description != null && node.contentDescription?.toString() == description) return node
            for (index in 0 until node.childCount) walk(node.getChild(index))?.let { return it }
            return null
        }
        if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow)
    }
    private fun waitFor(label: String, condition: () -> Boolean) {
        val deadline = SystemClock.uptimeMillis() + 10000
        while (!condition() && SystemClock.uptimeMillis() < deadline) SystemClock.sleep(80)
        assertTrue(label, condition())
    }
    private fun click(node: AccessibilityNodeInfo) {
        var current: AccessibilityNodeInfo? = node
        while (current != null) {
            if (current.actionList.any { it.id == AccessibilityNodeInfo.ACTION_CLICK }) {
                assertTrue(current.performAction(AccessibilityNodeInfo.ACTION_CLICK)); return
            }
            current = current.parent
        }
        error("No clickable ancestor")
    }
    @Test fun statusBadgesAndOfflineGeneratedImagePreviewRender() {
        val context = test.targetContext
        val conversation = "qa-native-session-images/owned"
        val source = Bitmap.createBitmap(512, 320, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.rgb(45, 110, 190)) }
        val bytes = ByteArrayOutputStream().also { source.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        source.recycle()
        val id = sha(bytes)
        val directory = File(context.filesDir, "generated-images/${sha(conversation.toByteArray())}").apply { mkdirs() }
        val file = File(directory, id).apply { writeBytes(bytes) }
        val engine = (context.applicationContext as KiteApplication).engine
        assertEquals(file, runBlocking { engine.generatedImage(conversation, id) })
        val decoded = decodeGeneratedImage(file)
        assertEquals(512, decoded.width); assertEquals(320, decoded.height); decoded.recycle()
        val activity = test.startActivitySync(Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        val hidden = mutableListOf<View>()
        lateinit var panel: ComposeView
        try {
            test.runOnMainSync {
                for (index in 0 until root.childCount) root.getChildAt(index).let { hidden.add(it); it.visibility = View.INVISIBLE }
                val vm = ViewModelProvider(activity)[HostCheckViewModel::class.java]
                panel = ComposeView(activity).apply { setContent {
                    KiteTheme { Column(Modifier.fillMaxWidth()) {
                        listOf(SessionStatus.Running, SessionStatus.Question, SessionStatus.Attention, SessionStatus.Disconnected).forEach { SessionStatusBadge(it) }
                        GeneratedImages(vm, listOf(AttachmentInfo(id, "generated-test.png", "image/png", bytes.size.toULong(), true)), conversation)
                    } }
                } }
                root.addView(panel, FrameLayout.LayoutParams(-1, -1))
            }
            for (label in listOf(R.string.session_running, R.string.session_question, R.string.session_attention, R.string.session_disconnected)) {
                waitFor(context.getString(label)) { find(text = context.getString(label)) != null }
            }
            val preview = context.getString(R.string.generated_image_preview)
            waitFor("image preview") { find(description = preview) != null }
            assertNotNull(find(text = context.getString(R.string.generated_image_save)))
            click(requireNotNull(find(description = preview)))
            waitFor("expanded image") { find(description = "generated-test.png") != null }
            click(requireNotNull(find(text = context.getString(R.string.dismiss))))
            waitFor("preview dismissed") { find(description = preview) != null }
        } finally {
            test.runOnMainSync { root.removeView(panel); panel.disposeComposition(); hidden.forEach { it.visibility = View.VISIBLE } }
            directory.deleteRecursively()
        }
    }
}
