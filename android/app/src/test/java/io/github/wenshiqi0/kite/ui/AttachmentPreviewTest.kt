package io.github.wenshiqi0.kite.ui

import org.junit.Assert.*
import org.junit.Test

class AttachmentPreviewTest {
    @Test fun textPreviewIsBoundedAndUnderstandsUtfBom() {
        val file=java.io.File.createTempFile("owned-preview-", ".txt")
        try {
            file.writeText("示例文本\n第二行")
            assertEquals(AttachmentText("示例文本\n第二行",false),readAttachmentText(file))
            file.writeBytes(byteArrayOf(0xff.toByte(),0xfe.toByte())+"中文文本".toByteArray(Charsets.UTF_16LE))
            assertEquals("中文文本",readAttachmentText(file).text)
            file.writeText("a".repeat(1000))
            val preview=readAttachmentText(file,100)
            assertEquals(100,preview.text.length);assertTrue(preview.truncated)
        } finally { file.delete() }
    }
    @Test fun imageAudioAndTextHaveNativePreviewKinds() {
        assertEquals(AttachmentPreviewKind.Image,attachmentPreviewKind("image/jpeg","screenshot.jpg"))
        assertEquals(AttachmentPreviewKind.Audio,attachmentPreviewKind("audio/mp4","recording.m4a"))
        assertEquals(AttachmentPreviewKind.Video,attachmentPreviewKind("video/mp4","clip.mp4"))
        assertEquals(AttachmentPreviewKind.Video,attachmentPreviewKind("application/octet-stream","clip.WEBM"))
        assertEquals(AttachmentPreviewKind.Audio,attachmentPreviewKind("application/octet-stream","recording.MP3"))
        assertEquals(AttachmentPreviewKind.Image,attachmentPreviewKind("application/octet-stream","photo.JPG"))
        assertEquals(AttachmentPreviewKind.Text,attachmentPreviewKind("text/plain","notes"))
        assertEquals(AttachmentPreviewKind.Text,attachmentPreviewKind("application/octet-stream","notes.md"))
        assertEquals(AttachmentPreviewKind.Text,attachmentPreviewKind("application/json","data"))
        assertEquals(AttachmentPreviewKind.Other,attachmentPreviewKind("application/zip","archive.zip"))
        assertEquals("0:00",previewTime(-10));assertEquals("1:05",previewTime(65000))
    }
}
