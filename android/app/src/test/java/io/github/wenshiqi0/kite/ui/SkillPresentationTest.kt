package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.ChatSkillInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ItemInfo
import io.github.wenshiqi0.kite.core.ItemRole
import org.junit.Assert.*
import org.junit.Test

class SkillPresentationTest {
    private val photos = ChatSkillInfo(
        name = "app-69312da8e4dc81919370cb86fd172b6c:adobe-batch-edit-photos",
        description = "Use this skill whenever the user wants visual cohesion",
        path = "/skills/photos/SKILL.md",
        displayName = "Batch edit photos",
        shortDescription = "Apply a consistent look across photos",
    )

    @Test fun friendlyMetadataIsShownAndSearchable() {
        assertEquals("Batch edit photos", photos.presentationName)
        assertEquals("Apply a consistent look across photos", photos.presentationDescription)
        assertTrue(photos.matchesSearch("BATCH EDIT"))
        assertTrue(photos.matchesSearch("consistent look"))
        assertTrue(photos.matchesSearch("app-69312"))
        assertTrue(photos.matchesSearch("visual cohesion"))
        assertFalse(photos.matchesSearch("mockups"))
    }

    @Test fun absentOrBlankMetadataFallsBackToOriginalFields() {
        for (empty in listOf(null, "", " \n\t")) {
            val legacy = photos.copy(displayName = empty, shortDescription = empty)
            assertEquals(photos.name, legacy.presentationName)
            assertEquals(photos.description, legacy.presentationDescription)
        }
    }

    @Test fun onlyPureSkillContextLeavesTheUserBubbleAndOrdinaryTextStaysUnchanged() {
        val skill = BlockInfo.SkillContext("report", "/skills/report/SKILL.md", "# Report")
        val item = ItemInfo("context", ItemRole.USER, listOf(skill), false, null)
        assertTrue(item.isSkillContextOnly)
        assertFalse(item.copy(blocks = emptyList()).isSkillContextOnly)
        val ordinary = BlockInfo.Text("\$report\n\nPlease check the tests.")
        val mixed = item.copy(blocks = listOf(skill, ordinary))
        assertFalse(mixed.isSkillContextOnly)
        assertEquals(ordinary.text, mixed.text)
        assertEquals(ordinary.text, item.copy(blocks = listOf(ordinary)).text)
    }
}
