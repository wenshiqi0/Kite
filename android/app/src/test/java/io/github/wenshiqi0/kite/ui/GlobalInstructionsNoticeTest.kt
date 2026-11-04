package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import org.junit.Assert.*
import org.junit.Test

class GlobalInstructionsNoticeTest {
    private fun message(text: String, kind: ItemRole = ItemRole.USER) = ItemInfo(
        id = "global", role = kind, blocks = listOf(io.github.wenshiqi0.kite.core.BlockInfo.Text(text)),
        intermediate = false, origin = null
    )
    private fun envelope(body: String) = "# AGENTS.md instructions\n\n<INSTRUCTIONS>\n$body\n</INSTRUCTIONS>"

    @Test fun originalAndReloadedGlobalGuidanceUseTheSameNoticeInEitherLanguage() {
        for (body in listOf("# 全局工作约束\n- 聚焦问题", "# Global Working Constraints\n- Stay focused.",
            "These AGENTS.md instructions replace all previously provided AGENTS.md instructions.\n# Global Working Constraints")) {
            assertTrue(isGlobalInstructionsMessage(message(envelope(body))))
            assertTrue(isGlobalInstructionsMessage(message("  " + envelope(body).replace("\n", "\r\n") + "\n")))
        }
    }

    @Test fun initialEnvironmentContextDoesNotExpandTheGlobalNotice() {
        val text = envelope("Global rules") + "\n<environment_context>\n<cwd>/workspace</cwd>\n</environment_context>"
        assertTrue(isGlobalInstructionsMessage(message(text)))
        assertFalse(isGlobalInstructionsMessage(message(envelope("Rules") + "\n<environment_context>")))
        assertFalse(isGlobalInstructionsMessage(message(text + "\nPlease fix the rendering.")))
        assertFalse(isGlobalInstructionsMessage(message(text + "\nPlease fix this.\n<environment_context></environment_context>")))
    }

    @Test fun ordinaryMessagesRepositoryGuidanceAndMixedRequestsRemainVisible() {
        for (text in listOf("Please update AGENTS.md instructions", "> " + envelope("Quoted rules"),
            envelope("Repository rules").replace("instructions\n", "instructions for /workspace\n"),
            "# AGENTS.md instructions\nPlease render this heading.",
            "# AGENTS.md instructions\n<INSTRUCTIONS>unfinished", envelope("Rules") + "\nAlso fix the UI.")) {
            assertFalse(text, isGlobalInstructionsMessage(message(text)))
        }
        assertFalse(isGlobalInstructionsMessage(message(envelope("Rules"), ItemRole.ASSISTANT)))
        assertFalse(isGlobalInstructionsMessage(message(envelope("Rules"), ItemRole.ERROR)))
    }
}
