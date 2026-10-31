package io.github.wenshiqi0.kite.media

import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.Surface
import android.util.Log
import androidx.annotation.Keep
import java.nio.ByteBuffer
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** MediaCodec owns decoding and Surface rendering. No network or pairing logic. */
@Keep
class VideoRenderer(private val surface: Surface) {
    private val buffers = Array(3) { ByteBuffer.allocateDirect(8 * 1024 * 1024) }
    private val closed = AtomicBoolean(false)
    private val guard = Any()
    @Volatile private var worker: Decoder? = null
    private val decoders by lazy {
        MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter {
            !it.isEncoder && it.supportedTypes.any { type -> type.equals(MediaFormat.MIMETYPE_VIDEO_AVC, ignoreCase = true) }
        }
    }
    val id = NativeMedia.register(this, buffers)

    init { check(id != 0L) { "native media registration failed" } }

    private fun decoderName(width: Int, height: Int, fps: Int): String? = decoders.firstOrNull { decoder ->
        runCatching {
            val caps = decoder.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities ?: return@runCatching false
            val alignedWidth = (width + caps.widthAlignment - 1) / caps.widthAlignment * caps.widthAlignment
            val alignedHeight = (height + caps.heightAlignment - 1) / caps.heightAlignment * caps.heightAlignment
            caps.areSizeAndRateSupported(alignedWidth, alignedHeight, fps.toDouble())
        }.getOrDefault(false)
    }?.name

    @Keep
    fun supportsFormat(width: Int, height: Int, fps: Int): Boolean =
        !closed.get() && width > 0 && height > 0 && decoderName(width, height, fps) != null

    @Keep
    fun onFormat(width: Int, height: Int, fps: Int, generation: Long): Boolean {
        if (closed.get() || !surface.isValid) return false
        val next = synchronized(guard) {
            worker?.let { it.stopDecoder(); it.join(1500) }
            Decoder(width, height, fps, generation).also { worker = it; it.start() }
        }
        return next.ready.await(5, TimeUnit.SECONDS) && next.configured && !closed.get()
    }

    @Keep
    fun onFrame(index: Int, size: Int, pts: Long, flags: Int): Boolean {
        val current = worker ?: return false
        return !closed.get() && !current.stopping.get() && current.frames.offer(Frame(index, size, pts, flags))
    }

    @Keep
    fun onStopped() { worker?.stopDecoder() }

    fun release() {
        if (closed.compareAndSet(false, true)) {
            worker?.stopDecoder()
            DesktopInput.releaseInput(id)
            NativeMedia.detach(id)
        }
    }

    private data class Frame(val index: Int, val size: Int, val pts: Long, val flags: Int)

    private inner class Decoder(val width: Int, val height: Int, val fps: Int, val generation: Long) : Thread("kite-video") {
        val frames = ArrayBlockingQueue<Frame>(3)
        val ready = CountDownLatch(1)
        val stopping = AtomicBoolean(false)
        @Volatile var configured = false

        fun stopDecoder() { stopping.set(true); interrupt() }

        override fun run() {
            var codec: MediaCodec? = null
            try {
                val name = decoderName(width, height, fps) ?: error("decoder size unsupported")
                val decoder = MediaCodec.createByCodecName(name)
                codec = decoder
                val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
                    setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 8 * 1024 * 1024)
                    setInteger(MediaFormat.KEY_FRAME_RATE, fps)
                    if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                }
                decoder.configure(format, surface, null, 0)
                decoder.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)
                Log.i("KiteVideo", "decoder=$name size=${width}x${height} fps=$fps")
                decoder.setOnFrameRenderedListener({ _, _, _ ->
                    NativeMedia.frameRendered(this@VideoRenderer.id, generation)
                }, Handler(Looper.getMainLooper()))
                decoder.start()
                configured = true
                ready.countDown()
                val info = MediaCodec.BufferInfo()
                while (!stopping.get() && !closed.get()) {
                    val frame = frames.poll(5, TimeUnit.MILLISECONDS)
                    if (frame != null) {
                        try {
                            val input = decoder.dequeueInputBuffer(5000)
                            if (input >= 0) {
                                val target = decoder.getInputBuffer(input) ?: error("decoder input unavailable")
                                target.clear()
                                if (frame.size > target.remaining()) error("decoder input too small")
                                val source = buffers[frame.index]
                                source.position(0); source.limit(frame.size); target.put(source)
                                val flags = when {
                                    frame.flags and 1 != 0 -> MediaCodec.BUFFER_FLAG_CODEC_CONFIG
                                    frame.flags and 2 != 0 -> MediaCodec.BUFFER_FLAG_KEY_FRAME
                                    else -> 0
                                }
                                decoder.queueInputBuffer(input, 0, frame.size, frame.pts, flags)
                            } else NativeMedia.requestIdr(this@VideoRenderer.id)
                        } finally { NativeMedia.releaseBuffer(this@VideoRenderer.id, frame.index) }
                    }
                    repeat(8) {
                        val output = decoder.dequeueOutputBuffer(info, 0)
                        if (output >= 0) decoder.releaseOutputBuffer(output, true)
                    }
                }
            } catch (_: InterruptedException) {
                // Normal Surface/lifecycle shutdown.
            } catch (_: Exception) {
                if (!stopping.get() && !closed.get()) NativeMedia.decoderFailed(this@VideoRenderer.id, generation)
            } finally {
                ready.countDown()
                stopping.set(true)
                while (true) {
                    val pending = frames.poll() ?: break
                    NativeMedia.releaseBuffer(this@VideoRenderer.id, pending.index)
                }
                try { codec?.stop() } catch (_: Exception) { }
                try { codec?.release() } catch (_: Exception) { }
            }
        }
    }
}
