package io.github.wenshiqi0.kite.media

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** The currently negotiated Moonlight format is 48kHz coupled stereo. */
object OpusStreamFormat {
    fun identification(rate:Int,channels:Int):ByteBuffer {
        require(rate==48000&&channels==2)
        return ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN).apply {
            put("OpusHead".toByteArray(Charsets.US_ASCII));put(1);put(channels.toByte())
            putShort(0);putInt(rate);putShort(0);put(0);flip()
        }
    }
    fun nativeLong(value:Long):ByteBuffer=ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).apply{putLong(value);flip()}
}
