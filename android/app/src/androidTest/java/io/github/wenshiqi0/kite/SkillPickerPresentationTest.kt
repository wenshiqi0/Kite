package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.*
import androidx.compose.ui.platform.ComposeView
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ChatSkillInfo
import io.github.wenshiqi0.kite.core.SkillDraftInfo
import io.github.wenshiqi0.kite.ui.SkillDrafts
import io.github.wenshiqi0.kite.ui.SkillPicker
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Test

/** Only synthetic skill metadata; never submits a provider turn. */
class SkillPickerPresentationTest {
    private val test get() = InstrumentationRegistry.getInstrumentation()
    private fun main(block: () -> Unit) { test.runOnMainSync(block) }
    private fun find(text: String? = null, description: String? = null, editable: Boolean = false): AccessibilityNodeInfo? {
        fun walk(node: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
            node ?: return null
            if (text != null && node.text?.toString()?.contains(text) == true || description != null && node.contentDescription?.toString() == description || editable && node.isEditable) return node
            for (i in 0 until node.childCount) walk(node.getChild(i))?.let { return it }
            return null
        }
        if (Build.VERSION.SDK_INT >= 33) test.uiAutomation.clearCache()
        return walk(test.uiAutomation.rootInActiveWindow)
    }
    private fun waitFor(label: String, condition: () -> Boolean) {
        val end = SystemClock.uptimeMillis() + 5000
        while (!condition() && SystemClock.uptimeMillis() < end) SystemClock.sleep(50)
        assertTrue(label, condition())
    }
    private fun click(node: AccessibilityNodeInfo) {
        var target: AccessibilityNodeInfo? = node
        while (target != null && !target.isClickable) target = target.parent
        assertTrue(requireNotNull(target).performAction(AccessibilityNodeInfo.ACTION_CLICK))
        test.waitForIdleSync()
    }
    private fun overlay(content: @Composable () -> Unit, check: () -> Unit) {
        val activity = test.startActivitySync(Intent(test.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val root = activity.findViewById<FrameLayout>(android.R.id.content)
        val original = mutableListOf<View>()
        lateinit var panel: ComposeView
        try {
            main {
                for (i in 0 until root.childCount) root.getChildAt(i).let { original.add(it); it.visibility = View.INVISIBLE }
                panel = ComposeView(activity).apply { setContent { KiteTheme { content() } } }
                root.addView(panel, FrameLayout.LayoutParams(-1, -1))
            }
            test.waitForIdleSync()
            check()
        } finally {
            main { panel.disposeComposition(); root.removeView(panel); original.forEach { it.visibility = View.VISIBLE }; activity.finish() }
        }
    }

    @Test fun descriptionsSearchMultiSelectAndChipRemoval() {
        val report = ChatSkillInfo("report", "生成项目进展报告", "/owned/report/SKILL.md")
        val review = ChatSkillInfo("review", "检查代码中的问题", "/owned/review/SKILL.md")
        var draft by mutableStateOf(SkillDraftInfo(listOf(report, review), emptyList(), true, false, ""))
        var open by mutableStateOf(true)
        overlay({
            Column {
                SkillDrafts(draft.selected, true) { skill -> draft = draft.copy(selected = draft.selected.filterNot { it.path == skill.path }) }
                if (open) SkillPicker(draft, true, { skill, selected ->
                    draft = draft.copy(selected = if (selected) draft.selected + skill else draft.selected.filterNot { it.path == skill.path })
                }, {}, { open = false })
            }
        }) {
            waitFor("Skill description") { find(text = "生成项目进展报告") != null }
            click(requireNotNull(find(text = "report")))
            click(requireNotNull(find(text = "review")))
            waitFor("Two selected skills") { find(text = "已选择 2 个 Skill") != null }
            val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "报告") }
            assertTrue(requireNotNull(find(editable = true)).performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args))
            test.waitForIdleSync()
            assertNotNull(find(text = "生成项目进展报告"))
            assertNull(find(text = "检查代码中的问题"))
            click(requireNotNull(find(text = "完成")))
            waitFor("Selected skill chips") { find(description = "取消选择 report") != null }
            click(requireNotNull(find(description = "取消选择 report")))
            waitFor("Removed chip") { find(description = "取消选择 report") == null }
            assertNotNull(find(description = "取消选择 review"))
        }
    }

    @Test fun emptyLoadingAndFailureAreDistinct() {
        val cases = listOf(
            SkillDraftInfo(emptyList(), emptyList(), true, false, "") to "当前会话没有可用的 Skill",
            SkillDraftInfo(emptyList(), emptyList(), false, true, "") to "正在加载 Skills",
            SkillDraftInfo(emptyList(), emptyList(), false, false, "owned failure") to "Skills 加载失败",
        )
        cases.forEach { (draft, label) ->
            overlay({ SkillPicker(draft, true, { _, _ -> }, {}, {}) }) {
                waitFor(label) { find(text = label) != null }
            }
        }
    }
}
