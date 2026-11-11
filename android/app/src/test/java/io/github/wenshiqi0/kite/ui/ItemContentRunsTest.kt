package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ToolInfo
import org.junit.Assert.*
import org.junit.Test

class ItemContentRunsTest {
    private fun image(id: String) = BlockInfo.Attachment(AttachmentInfo(id, "$id.png", "image/png", 10uL, false))
    @Test fun consecutiveImagesShareAnAlbumAcrossBlankMarkdownSeparators() {
        val images = listOf(image("a"), image("b"), image("c"))
        val runs = itemContentRuns(listOf(images[0], BlockInfo.Text("\n"), BlockInfo.Text("  "), images[1], BlockInfo.Text("\n\n"), images[2]))
        assertEquals(1, runs.size)
        assertEquals(images, runs.single().blocks)
        assertEquals(0, runs.single().start)
    }
    @Test fun textAndToolBoundariesKeepTheirOriginalPositions() {
        val call = BlockInfo.Tool(ToolInfo("Bash", "owned", "exec_command", "{}", "ok", true, false, false))
        val blocks = listOf(BlockInfo.Text("before"), image("a"), BlockInfo.Text("caption"), image("b"), call, image("c"))
        val runs = itemContentRuns(blocks)
        assertEquals(blocks, runs.flatMap { it.blocks })
        assertEquals(blocks.indices.toList(), runs.map { it.start })
        assertTrue(itemContentRuns(emptyList()).isEmpty())
    }
}
