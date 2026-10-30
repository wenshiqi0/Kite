package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test

class RemoteTextBufferTest {
    @Test fun continuous_input_uses_one_fixed_window_and_explicit_flush_preserves_order(){
        val sent=mutableListOf<String>();val tasks=mutableListOf<()->Unit>()
        val buffer=RemoteTextBuffer({sent.add(it);true},{tasks.add(it)},{fail("send failed")})
        buffer.append("a");buffer.append("你");buffer.append("b")
        assertEquals(1,tasks.size);assertTrue(sent.isEmpty())
        tasks.removeAt(0).invoke();assertEquals(listOf("a你b"),sent)
        buffer.append("next");buffer.flush();tasks.forEach{it()};assertEquals(listOf("a你b","next"),sent)
    }
    @Test fun deletion_keeps_unicode_boundaries_and_stale_callbacks_never_replay_text(){
        val sent=mutableListOf<String>();val tasks=mutableListOf<()->Unit>()
        val buffer=RemoteTextBuffer({sent.add(it);true},{tasks.add(it)},{fail("send failed")})
        buffer.append("abc😀");assertEquals(2,buffer.removeBefore(1));buffer.flush()
        assertEquals(listOf("abc"),sent)
        buffer.append("cancelled");buffer.cancel();tasks.forEach{it()};assertEquals(listOf("abc"),sent)
    }
    @Test fun long_paste_is_split_into_bounded_utf8_packets(){
        val sent=mutableListOf<String>();val text="你😀".repeat(2000)
        val buffer=RemoteTextBuffer({sent.add(it);true},{},{fail("send failed")})
        buffer.append(text);buffer.flush()
        assertEquals(text,sent.joinToString(""));assertTrue(sent.all{it.toByteArray(Charsets.UTF_8).size<=4096})
    }
}
