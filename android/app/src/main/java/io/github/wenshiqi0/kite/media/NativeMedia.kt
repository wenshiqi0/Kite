package io.github.wenshiqi0.kite.media

import java.nio.ByteBuffer

/** The high frequency media/input channel, separate from UniFFI. */
object NativeMedia {
    init { System.loadLibrary("kite_ffi") }
    @JvmStatic external fun register(renderer: VideoRenderer, buffers: Array<ByteBuffer>): Long
    @JvmStatic external fun detach(id: Long)
    @JvmStatic external fun releaseBuffer(id: Long, index: Int)
    @JvmStatic external fun frameRendered(id: Long, generation: Long)
    @JvmStatic external fun decoderFailed(id: Long, generation: Long)
    @JvmStatic external fun requestIdr(id: Long)
    @JvmStatic external fun mousePosition(id: Long, x: Int, y: Int, width: Int, height: Int): Boolean
    @JvmStatic external fun mouseButton(id: Long, button: Int, down: Boolean): Boolean
    @JvmStatic external fun scroll(id: Long, horizontal: Int, vertical: Int): Boolean
    @JvmStatic external fun key(id: Long, key: Int, down: Boolean, modifiers: Int): Boolean
    @JvmStatic external fun text(id: Long, text: String): Boolean
    @JvmStatic external fun releaseInput(id: Long)
}
