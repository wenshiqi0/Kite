package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class MarkdownNormalizationTest {
    @Test fun cjkSentencePunctuationCanCloseStrongBeforeTheNextWord() {
        assertEquals("**已经完成。** Kite 接下来继续。", normalizeCjkStrong("**已经完成。**Kite 接下来继续。"))
        assertEquals("**结论：** 下一步", normalizeCjkStrong("**结论：**下一步"))
        assertEquals("此前。**重点**Kite", normalizeCjkStrong("此前。**重点**Kite"))
    }
    @Test fun codeAndEscapedLiteralAsterisksKeepTheirExactSource() {
        for (source in listOf("`**已经完成。**Kite`", "```md\n**已经完成。**Kite\n```", "~~~\n**已经完成。**Kite\n~~~", "    **已经完成。**Kite", "\\*\\*已经完成。\\*\\*Kite", "[文件](https://example.com/**完成。**Kite)")) {
            assertEquals(source, normalizeCjkStrong(source))
        }
    }
    @Test fun ordinaryStrongLinksAndUnfinishedStreamingTextArePreserved() {
        for (source in listOf("**正常粗体**", "**等待回答**：发送提醒", "[说明](https://example.com)", "**尚未完成", "2 ** 3", "***三重强调***")) assertEquals(source, normalizeCjkStrong(source))
    }
}
