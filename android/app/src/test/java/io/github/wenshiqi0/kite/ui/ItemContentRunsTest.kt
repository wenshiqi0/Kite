package io.github.wenshiqi0.kite.ui

import io.github.wenshiqi0.kite.core.AttachmentInfo
import io.github.wenshiqi0.kite.core.BlockInfo
import io.github.wenshiqi0.kite.core.ToolInfo
import org.junit.Assert.*
import org.junit.Test

class ItemContentRunsTest {
    private fun image(id: String) = BlockInfo.Attachment(AttachmentInfo(id, "$id.png", "image/png", 10uL, false))
    private fun document(id: String, name: String = "$id.md", mime: String = "text/markdown") =
        BlockInfo.Attachment(AttachmentInfo(id, name, mime, 10uL, false))

    @Test fun fileReferencesKeepTheSentenceAndListInOneMarkdownRun() {
        val blocks = listOf(
            BlockInfo.Text("已更新 "), document("plan"), BlockInfo.Text("，请确认：\n\n- **推荐**：把 "),
            document("config", "skills.json", "application/json"), BlockInfo.Text(" 切换为 `project/skills`。\n- 保留原目录。"),
        )
        val runs = itemContentRuns(blocks)
        assertEquals(1, runs.size)
        assertEquals(blocks, runs.single().blocks)
        assertEquals("已更新 [plan\\.md](kite-attachment:0)，请确认：\n\n- **推荐**：把 [skills\\.json](kite-attachment:1) 切换为 `project/skills`。\n- 保留原目录。", inlineFileMarkdown(runs.single().blocks))
    }

    @Test fun documentRunsStopAtMediaAndControlBlocks() {
        val call = BlockInfo.Tool(ToolInfo("Bash", "owned", "exec_command", "{}", "ok", true, false, false))
        val audio = BlockInfo.Attachment(AttachmentInfo("audio", "clip.wav", "audio/wav", 10uL, false))
        val video = BlockInfo.Attachment(AttachmentInfo("video", "clip.mp4", "video/mp4", 10uL, false))
        val blocks = listOf(BlockInfo.Text("before "), document("a"), image("photo"),
            document("b", "report.pdf", "application/pdf"), audio, video, call, BlockInfo.Text("after"))
        val runs = itemContentRuns(blocks)
        assertEquals(blocks, runs.flatMap { it.blocks })
        assertEquals(listOf(0, 2, 3, 4, 6, 7), runs.map { it.start })
        assertEquals(listOf(audio, video), runs[3].blocks)
    }

    @Test fun adjacentFilesHaveASeparatorAndEscapedLabels() {
        val blocks = listOf(document("first", "draft [v2](final)*_`\\.md"), document("second", "另一个.json"))
        val markdown = inlineFileMarkdown(itemContentRuns(blocks).single().blocks)
        assertEquals("[draft \\[v2\\]\\(final\\)\\*\\_\\`\\\\\\.md](kite-attachment:0) [另一个\\.json](kite-attachment:1)", markdown)
    }
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
