package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.*
import io.github.wenshiqi0.kite.ui.theme.KiteColors
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking

/** Owned display records only. No host, credentials, model turn or real conversation is used. */
class AssociatedTaskPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private val task = SubagentInfo("owned-task", "owned-origin", "Fling validation", "fling-check", "done", 0)
    private val history = SubagentInspectionInfo((0..79).map { index ->
        ItemInfo("owned-message-$index", ItemRole.ASSISTANT,
            listOf(BlockInfo.Text("Owned record $index\n\n" + (1..4).joinToString("\n") { "Public task detail line $it." })), false, null)
    }, true)

    @Test fun relatedTasksStayFlatCompactAndKeepTheirOriginalReadTargets() {
        val tasks = listOf(
            SubagentInfo("owned-linked", "owned-worker", "Verifier", "research/verify", "done", 1),
            SubagentInfo("owned-worker", "owned-origin", "Worker", "research", "working", 2),
            SubagentInfo("owned-audit", "owned-origin", "Auditor", "audit", "failed", 3),
        )
        val selected = mutableStateOf<SubagentInfo?>(null)
        overlay({
            Column(Modifier.fillMaxWidth().padding(20.dp)) {
                Text("Original conversation remains selected")
                AssociatedTasks("owned-flat-list", tasks, false, true, inspect = { selected.value = it })
            }
        }) {
            waitFor("Compact folded related task entry") { find(text = "关联任务 · 3") != null }
            assertNull(find(text = "verify"))
            val density = test.targetContext.resources.displayMetrics.density
            val header = actionNode(requireNotNull(find(text = "关联任务 · 3")), AccessibilityNodeInfo.ACTION_CLICK)
            val touch = Rect().also(header::getBoundsInScreen)
            assertTrue("Entry retains a 48dp touch height", touch.height() / density >= 48f)
            assertNotNull(find(text = "1 个已完成"))
            assertTrue(header.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Tasks at both provider depths are peers") { find(text = "verify") != null && find(text = "research") != null }
            assertEquals(bounds(text = "verify").left, bounds(text = "research").left)
            assertTrue("Expanded rows stay within 96dp", bounds(description = "关联任务").height() / density <= 96f + 1f)
            assertNull(find(description = "展开 Worker 的子 agent"))
            click(text = "verify")
            assertEquals("Inspection retains the actual provider thread and source edge", tasks[0], selected.value)
            val list = requireNotNull(find(description = "关联任务"))
            assertTrue(list.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD))
            waitFor("Every task remains reachable in the bounded list") { find(text = "audit") != null }
            assertNotNull(find(text = "运行失败"))
            click(text = "audit")
            assertEquals(tasks[2], selected.value)
            click(text = "关联任务 · 3")
            waitFor("One fold hides the flat task list") { find(text = "audit") == null }
            assertNotNull(find(text = "Original conversation remains selected"))
            screenshot("associated-tasks-compact.png")
        }
    }

    @Test fun readonlyDetailsExposeMessagesToolsAndResultsWithoutChangingTheConversation() {
        val open = mutableStateOf(true)
        val reads = AtomicInteger()
        val records = SubagentInspectionInfo(listOf(
            ItemInfo("owned-recent", ItemRole.ASSISTANT, listOf(BlockInfo.Text("Owned recent message")), false, null),
            ItemInfo("owned-tool", ItemRole.TOOL, listOf(BlockInfo.Tool(ToolInfo("Bash", "Owned process", "exec_command", "owned --read-only", "Owned tool output", true, false, false))), false, null),
            ItemInfo("owned-final", ItemRole.ASSISTANT, listOf(BlockInfo.Text("Owned final result")), false, null),
        ), true)
        overlay({
            Text("Original conversation remains selected")
            if (open.value) AssociatedTaskInspector(task, true, load = { reads.incrementAndGet(); records }, dismiss = { open.value = false })
        }) {
            waitFor("Readonly final result") { find(text = "Owned final result") != null }
            assertNotNull(find(text = "关联任务"))
            assertNotNull(find(text = "只读 · 最近记录"))
            assertNotNull(find(text = "Owned recent message"))
            assertNull(find(description = "消息输入框"))
            SystemClock.sleep(900)
            val metrics = test.targetContext.resources.displayMetrics
            assertTrue("Details leave at least a quarter of the workspace above", bounds(text = "关联任务").top > metrics.heightPixels * .25f)
            click(description = "展开 1 次工具调用")
            waitFor("Tool process can be expanded") { find(text = "Bash · Owned process") != null }
            click(text = "Bash · Owned process")
            waitFor("Readonly arguments and output") { find(text = "owned --read-only") != null && find(text = "Owned tool output") != null }
            click(text = "关闭")
            click(text = "刷新记录")
            waitFor("Refresh only reads the same task") { reads.get() == 2 }
            screenshot("associated-task-readonly.png")
            click(description = "关闭")
            waitFor("Dismiss returns to the unchanged conversation") { !open.value && find(text = "Original conversation remains selected") != null }
        }
    }

    @Test fun refreshingDoesNotPullAReaderBackToTheLatestMessage() {
        val reads = AtomicInteger()
        overlay({ AssociatedTaskInspector(task, true, load = {
            if (reads.incrementAndGet() == 1) history else history.copy(items = history.items +
                ItemInfo("owned-new-result", ItemRole.ASSISTANT, listOf(BlockInfo.Text("Owned refreshed final result")), false, null))
        }, dismiss = {}) }) {
            waitFor("Latest result is initially visible") { find(text = "Owned record 79") != null }
            SystemClock.sleep(700)
            fastSwipe(bounds(description = "只读记录"), upward = false)
            waitForStableMessage()
            assertNull("The fast downward history gesture must reach older messages", find(text = "Owned record 79"))
            val reading = requireNotNull(find(text = "Owned record ")).text.toString()
            val before = bounds(text = reading)
            val headerBefore = bounds(text = task.name)
            click(text = "刷新记录")
            waitFor("Refresh completes") { reads.get() == 2 && requireNotNull(find(text = "刷新记录")).isEnabled }
            SystemClock.sleep(300)
            assertNotNull("Refresh keeps the same message visible", find(text = reading))
            File(test.targetContext.getExternalFilesDir(null), "associated-task-refresh-position.json").writeText("""{"beforeTop":${before.top},"afterTop":${bounds(text = reading).top},"headerBefore":${headerBefore.top},"headerAfter":${bounds(text = task.name).top}}""")
            assertEquals(before.top.toFloat(), bounds(text = reading).top.toFloat(), 1f)
            assertNull("Refresh does not jump to the appended final result", find(text = "Owned refreshed final result"))
            screenshot("associated-task-refresh-reading.png")
        }
    }

    @Test fun fastUpwardFlingAtTheHistoryEndSettlesWithoutMovingTheSheetAgain() {
        overlay({ AssociatedTaskInspector(task, true, load = { history }, dismiss = {}) }) {
            waitFor("Recent task result is initially visible") { find(text = "Owned record 79") != null }
            SystemClock.sleep(1200)
            val header = bounds(text = task.name)
            val list = bounds(description = "只读记录")
            val positions = mutableListOf<Int>()
            repeat(3) {
                fastSwipe(list, upward = true)
                repeat(100) {
                    positions += bounds(text = task.name).top
                    SystemClock.sleep(40)
                }
                val settled = positions.takeLast(50)
                File(test.targetContext.getExternalFilesDir(null), "associated-task-fling-$it.json")
                    .writeText("""{"initialTop":${header.top},"headerPositions":$positions}""")
                assertTrue("Residual upward velocity must not move an already expanded sheet: $positions", positions.max() - positions.min() <= 1)
                assertTrue("Sheet must settle after a fast upward fling: $settled", settled.max() - settled.min() <= 1)
                assertEquals("Upward list flings retain the expanded sheet anchor", header.top.toFloat(), settled.last().toFloat(), 1f)
            }
            screenshot("associated-task-fling-settled.png")
        }
    }

    @Test fun fastFlingThroughAnExpandedToolListAlsoSettlesAtTheSheetBoundary() {
        val tools = (0..39).map { index ->
            ItemInfo("owned-tool-$index", ItemRole.TOOL, listOf(BlockInfo.Tool(
                ToolInfo("Bash", "Owned tool $index", "exec_command", "owned command", "owned output", true, false, false)
            )), false, null)
        }
        val records = SubagentInspectionInfo(tools + ItemInfo("owned-tool-result", ItemRole.ASSISTANT, listOf(BlockInfo.Text("Owned tool final result")), false, null), false)
        overlay({ AssociatedTaskInspector(task, true, load = { records }, dismiss = {}) }) {
            waitFor("Task tool group and final result are visible") { find(description = "展开 40 次工具调用") != null && find(text = "Owned tool final result") != null }
            click(description = "展开 40 次工具调用")
            waitFor("Expanded nested tool list") { find(text = "Owned tool 0") != null }
            val firstTool = requireNotNull(find(text = "Owned tool 0"))
            val toolsList = actionNode(firstTool, AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)
            repeat(10) { toolsList.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD); SystemClock.sleep(120) }
            waitFor("Nested list reaches the final tool") { find(text = "Owned tool 39") != null }
            val rect = Rect().also(toolsList::getBoundsInScreen)
            SystemClock.sleep(500)
            val top = bounds(text = task.name).top
            repeat(3) { fastSwipe(rect, upward = true); SystemClock.sleep(80) }
            val positions = (1..100).map { bounds(text = task.name).top.also { SystemClock.sleep(40) } }
            File(test.targetContext.getExternalFilesDir(null), "associated-task-nested-fling.json").writeText("""{"headerPositions":$positions}""")
            assertTrue("Nested flings must settle without repeatedly moving the sheet", positions.takeLast(50).all { kotlin.math.abs(it - top) <= 1 })
            screenshot("associated-task-nested-fling.png")
        }
    }

    @Test fun realWorkspaceTailFollowingSettlesAfterFastFlings() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("qa_native_history") == "true")
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm: HostCheckViewModel
        main { vm = ViewModelProvider(activity)[HostCheckViewModel::class.java] }
        waitFor("Core initialized") { vm.engine.state.value?.initialized == true }
        assumeTrue("The workspace fixture requires an empty, isolated app", vm.engine.state.value?.hosts?.isEmpty() == true)
        val saved = runBlocking { vm.engine.core.conversationSince("owned-fling-workspace", 0uL) }
        val context = requireNotNull(vm.engine.state.value).conversationContext
        val records = (0..80).map { index -> ItemInfo("owned-workspace-$index", ItemRole.ASSISTANT,
            listOf(BlockInfo.Text(if (index == 80) "Owned workspace tail" else "Owned workspace record $index\n\n" + (1..8).joinToString("\n\n") { "Owned paragraph $it for fast-fling verification." })), false, null) }
        try {
            main {
                vm.settingsVisible = false
                vm.engine.timeline.prepareContext(context)
                vm.engine.timeline.apply(ConversationDeltaInfo(context, 100000uL, true, listOf(ConversationEditInfo(0uL, 0uL, records))))
            }
            waitFor("Actual workspace follows its tail") { find(text = "Owned workspace tail") != null }
            SystemClock.sleep(700)
            val rect = bounds(description = "聊天记录")
            repeat(3) { fastSwipe(rect, upward = true); SystemClock.sleep(80) }
            val positions = (1..100).map { bounds(text = "Owned workspace tail").top.also { SystemClock.sleep(40) } }
            File(test.targetContext.getExternalFilesDir(null), "associated-task-workspace-fling.json").writeText("""{"tailPositions":$positions}""")
            val settled = positions.takeLast(50)
            assertTrue("Automatic tail following must stop after a fling", settled.max() - settled.min() <= 1)
            assertEquals(records.size, vm.engine.timeline.messages.size)
            screenshot("associated-task-workspace-fling.png")
        } finally {
            main { vm.engine.timeline.prepareContext(saved.context); vm.engine.timeline.apply(saved); activity.finish() }
        }
    }

    @Test fun downwardHandleFlingStillDismissesTheDetails() {
        val open = mutableStateOf(true)
        overlay({
            Text("Original conversation remains selected")
            if (open.value) AssociatedTaskInspector(task, true, load = { history }, dismiss = { open.value = false })
        }) {
            waitFor("Task details loaded") { find(text = "Owned record 79") != null }
            SystemClock.sleep(700)
            val header = bounds(text = "关联任务")
            val metrics = test.targetContext.resources.displayMetrics
            fastSwipe(Rect(40, header.top - 32, metrics.widthPixels - 40, metrics.heightPixels - 100), upward = false,
                startY = header.top - 24 * metrics.density)
            waitFor("Downward sheet gesture still dismisses and returns to the conversation") {
                !open.value && find(text = "Original conversation remains selected") != null
            }
        }
    }

    private fun fastSwipe(rect: Rect, upward: Boolean, startY: Float? = null) {
        val down = SystemClock.uptimeMillis()
        val from = startY ?: if (upward) rect.bottom - rect.height() * .1f else rect.top + rect.height() * .1f
        val to = if (upward) rect.top + rect.height() * .1f else rect.bottom - rect.height() * .1f
        fun send(action: Int, y: Float) {
            val event = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, rect.exactCenterX(), y, 0)
            event.source = InputDevice.SOURCE_TOUCHSCREEN
            try { assertTrue(test.uiAutomation.injectInputEvent(event, action == MotionEvent.ACTION_DOWN)) } finally { event.recycle() }
        }
        send(MotionEvent.ACTION_DOWN, from)
        for (step in 1..6) {
            SystemClock.sleep(4)
            send(MotionEvent.ACTION_MOVE, from + (to - from) * step / 6)
        }
        // Lift immediately to retain the fling velocity; do not turn this into a slow drag.
        send(MotionEvent.ACTION_UP, to)
    }

    private fun main(block: () -> Unit) {
        var error: Throwable? = null
        test.runOnMainSync { try { block() } catch (caught: Throwable) { error = caught } }
        error?.let { throw it }
    }

    private fun waitFor(label: String, condition: () -> Boolean) {
        val end = SystemClock.uptimeMillis() + 15000
        while (!condition() && SystemClock.uptimeMillis() < end) SystemClock.sleep(60)
        assertTrue(label, condition())
    }

    private fun waitForStableMessage() {
        var previous: Pair<String, Int>? = null
        var unchanged = 0
        waitFor("The history fling stops before testing a refresh") {
            val node = find(text = "Owned record ")
            val next = node?.let { it.text.toString() to Rect().also(it::getBoundsInScreen).top }
            unchanged = if (next != null && next == previous) unchanged + 1 else 0
            previous = next
            unchanged >= 8
        }
    }

    private fun find(text: String? = null, description: String? = null): AccessibilityNodeInfo? {
        fun walk(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            node ?: return null
            if (node.isVisibleToUser && (text != null && node.text?.contains(text) == true ||
                    description != null && node.contentDescription?.toString() == description)) return node
            for (index in 0 until node.childCount) walk(node.getChild(index))?.let { return it }
            return null
        }
        if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow)
    }

    private fun bounds(text: String? = null, description: String? = null) = Rect().also {
        requireNotNull(find(text, description)) { "Missing node: $text / $description" }.getBoundsInScreen(it)
    }

    private fun actionNode(node: AccessibilityNodeInfo, action: Int): AccessibilityNodeInfo {
        var current: AccessibilityNodeInfo? = node
        while (current != null) {
            if (current.actionList.any { it.id == action }) return current
            current = current.parent
        }
        error("No actionable ancestor for ${node.text}: $action")
    }

    private fun click(text: String? = null, description: String? = null) {
        waitFor("Action is visible: $text / $description") { find(text, description) != null }
        assertTrue(actionNode(requireNotNull(find(text, description)), AccessibilityNodeInfo.ACTION_CLICK).performAction(AccessibilityNodeInfo.ACTION_CLICK))
    }

    private fun screenshot(name: String) {
        val bitmap = test.uiAutomation.takeScreenshot()
        File(test.targetContext.getExternalFilesDir(null), name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    private fun overlay(content: @Composable () -> Unit, check: () -> Unit) {
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        val original = mutableListOf<View>()
        lateinit var panel: ComposeView
        try {
            main {
                for (index in 0 until root.childCount) root.getChildAt(index).let { original += it; it.visibility = View.INVISIBLE }
                panel = ComposeView(activity).apply {
                    setContent { KiteTheme { Box(Modifier.fillMaxSize().background(KiteColors.Background)) { content() } } }
                }
                root.addView(panel, FrameLayout.LayoutParams(-1, -1))
            }
            test.waitForIdleSync()
            check()
        } catch (error: Throwable) {
            runCatching { screenshot("associated-task-failure.png") }
            throw error
        } finally {
            main { panel.disposeComposition(); root.removeView(panel); original.forEach { it.visibility = View.VISIBLE }; activity.finish() }
        }
    }
}
