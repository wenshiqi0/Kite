package io.github.wenshiqi0.kite.ui

import java.io.File

internal data class AttachmentText(val text: String, val truncated: Boolean)
internal fun readAttachmentText(file: File, limit: Int = 128*1024): AttachmentText = file.inputStream().buffered().use { source ->
    source.mark(3)
    val first=source.read();val second=source.read()
    source.reset()
    val charset=when {
        first==0xff&&second==0xfe -> Charsets.UTF_16LE
        first==0xfe&&second==0xff -> Charsets.UTF_16BE
        else -> Charsets.UTF_8
    }
    source.reader(charset).use { reader ->
        val chars=CharArray(limit+1)
        var count=0
        while(count<chars.size){val n=reader.read(chars,count,chars.size-count);if(n<0)break;count+=n}
        var end=minOf(count,limit)
        if(end>0&&Character.isHighSurrogate(chars[end-1]))end--
        AttachmentText(String(chars,0,end).removePrefix("\uFEFF"),count>limit)
    }
}
