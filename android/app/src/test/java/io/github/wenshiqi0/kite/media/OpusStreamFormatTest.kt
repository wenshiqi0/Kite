package io.github.wenshiqi0.kite.media

import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteOrder

class OpusStreamFormatTest {
    @Test fun stereoHeaderAndDelayBuffersMatchAndroidCodecContract(){
        val header=OpusStreamFormat.identification(48000,2)
        assertEquals(19,header.remaining())
        val magic=ByteArray(8);header.get(magic);assertEquals("OpusHead",String(magic,Charsets.US_ASCII))
        assertEquals(1,header.get().toInt());assertEquals(2,header.get().toInt())
        assertEquals(0,header.short.toInt());assertEquals(48000,header.int);assertEquals(0,header.short.toInt());assertEquals(0,header.get().toInt())
        val preroll=OpusStreamFormat.nativeLong(80_000_000);assertEquals(ByteOrder.nativeOrder(),preroll.order());assertEquals(80_000_000L,preroll.long)
    }
    @Test(expected=IllegalArgumentException::class)fun unsupportedChannelMappingCannotBeMislabelledAsStereo(){OpusStreamFormat.identification(48000,6)}
}
