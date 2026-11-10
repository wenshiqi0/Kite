package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Bitmap
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
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import io.github.wenshiqi0.kite.core.SkillDraftInfo
import io.github.wenshiqi0.kite.ui.ChatEntry
import io.github.wenshiqi0.kite.ui.ChatEntryContent
import io.github.wenshiqi0.kite.ui.SkillDrafts
import io.github.wenshiqi0.kite.ui.SkillPicker
import io.github.wenshiqi0.kite.ui.theme.KiteTheme
import org.junit.Assert.*
import org.junit.Test
import java.io.File

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
        val report = ChatSkillInfo(
            "app-test:report", "Use this skill whenever a progress report is requested", "/owned/report/SKILL.md",
            "项目进展报告", "汇总本周进展",
        )
        val review = ChatSkillInfo("review", "检查代码中的问题", "/owned/review/SKILL.md", null, null)
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
            waitFor("Skill description") { find(text = "汇总本周进展") != null }
            assertNotNull(find(text = "项目进展报告"))
            assertNull(find(text = "app-test:report"))
            assertNull(find(text = "Use this skill whenever"))
            click(requireNotNull(find(text = "项目进展报告")))
            click(requireNotNull(find(text = "review")))
            waitFor("Two selected skills") { find(text = "已选择 2 个 Skill") != null }
            val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, "报告") }
            assertTrue(requireNotNull(find(editable = true)).performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args))
            waitFor("Search filters the other skill") { find(text = "检查代码中的问题") == null }
            assertNotNull(find(text = "汇总本周进展"))
            assertNull(find(text = "检查代码中的问题"))
            click(requireNotNull(find(text = "完成")))
            waitFor("Selected skill chips") { find(description = "取消选择 项目进展报告") != null }
            click(requireNotNull(find(description = "取消选择 项目进展报告")))
            waitFor("Removed chip") { find(description = "取消选择 项目进展报告") == null }
            assertNotNull(find(description = "取消选择 review"))
        }
    }

    @Test fun pluginSkillsUseFriendlyNamesAndShortSummaries() {
        val skills = listOf(
            Triple("adobe-batch-edit-photos", "Batch edit photos", "Apply a consistent look across a set of photos."),
            Triple("adobe-create-mockups", "Create mockups", "Takes a logo or a design asset and produces a suite of mockups."),
            Triple("adobe-create-social-variations", "Create social variations", "Resize and export images into platform-ready social media sets."),
        ).map { (name, title, summary) ->
            ChatSkillInfo(
                "app-69312da8e4dc81919370cb86fd172b6c:$name",
                "Use this skill whenever the user asks for $name", "/owned/$name/SKILL.md", title, summary,
            )
        }
        overlay({ SkillPicker(SkillDraftInfo(skills, emptyList(), true, false, ""), true, { _, _ -> }, {}, {}) }) {
            waitFor("All plugin skills are visible") { find(text = "Create social variations") != null }
            skills.forEach { skill ->
                assertNotNull(find(text = skill.displayName))
                assertNotNull(find(text = skill.shortDescription))
            }
            assertNull(find(text = "app-69312"))
            assertNull(find(text = "Use this skill whenever"))
            val image = requireNotNull(test.uiAutomation.takeScreenshot())
            File(test.targetContext.getExternalFilesDir(null), "skills-friendly-names.png").outputStream().use {
                image.compress(Bitmap.CompressFormat.PNG, 100, it)
            }
            image.recycle()
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

    @Test fun transcriptSkillCardsFoldIndependentlyAndKeepTheUsersOwnTextVisible() {
        val message = ItemInfo("mixed-skills", ItemRole.USER, listOf(
            BlockInfo.Text("\$report：请检查测试。"),
            BlockInfo.SkillContext("report", "/owned/report/SKILL.md", "# 项目报告\n\n只处理当前项目的报告。"),
            BlockInfo.SkillContext("review", "/owned/review/SKILL.md", "# 代码检查\n\n检查回归风险。"),
            BlockInfo.Text("保留这条补充要求。"),
        ), false, null)
        overlay({ ChatEntryContent(ChatEntry.Message(message)) {} }) {
            waitFor("Skill context label") { find(text = "Skill 指令") != null }
            assertNotNull(find(text = "\$report：请检查测试。"))
            assertNotNull(find(text = "保留这条补充要求。"))
            assertNull(find(text = "只处理当前项目的报告。"))
            assertNull(find(text = "/owned/report/SKILL.md"))
            assertNull(find(text = "检查回归风险。"))
            click(requireNotNull(find(description = "展开 report 的 Skill 指令")))
            waitFor("Expanded Markdown body") { find(text = "只处理当前项目的报告。") != null }
            assertNotNull(find(text = "来源：/owned/report/SKILL.md"))
            assertNull(find(text = "检查回归风险。"))
            val screenshot = requireNotNull(test.uiAutomation.takeScreenshot())
            File(test.targetContext.getExternalFilesDir(null), "skill-context-expanded.png").outputStream().use {
                screenshot.compress(Bitmap.CompressFormat.PNG, 100, it)
            }
            screenshot.recycle()
            click(requireNotNull(find(description = "收起 report 的 Skill 指令")))
            waitFor("Collapsed body") { find(text = "只处理当前项目的报告。") == null }
            click(requireNotNull(find(description = "展开 review 的 Skill 指令")))
            waitFor("Second independent body") { find(text = "检查回归风险。") != null }
            assertNull(find(text = "只处理当前项目的报告。"))
            assertNotNull(find(text = "\$report：请检查测试。"))
            assertNotNull(find(text = "保留这条补充要求。"))
        }
    }
}
