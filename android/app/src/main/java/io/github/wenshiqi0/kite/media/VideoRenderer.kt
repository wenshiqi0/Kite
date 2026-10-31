package io.github.wenshiqi0.kite.media

import android.media.MediaCodec
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.Process
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
    val performance = VideoPerformance()
    private val decoders by lazy {
        val choices=MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter {
            !it.isEncoder && it.supportedTypes.any { type -> type.equals(MediaFormat.MIMETYPE_VIDEO_AVC, ignoreCase = true) }
        }
        val accelerated=choices.filter { decoder ->
            if(Build.VERSION.SDK_INT>=29)decoder.isHardwareAccelerated
            else !decoder.name.startsWith("OMX.google.")&&!decoder.name.startsWith("c2.android.")
        }
        (accelerated.ifEmpty{choices}).sortedBy { if(Build.VERSION.SDK_INT>=29&&it.isAlias)1 else 0 }
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
        val at=System.nanoTime()
        if(flags and 1==0)performance.received(pts,current.frames.size+1,at)
        val accepted=!closed.get()&&!current.stopping.get()&&current.frames.offer(Frame(index,size,pts,flags,at))
        if(!accepted)performance.rejected()
        return accepted
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

    private data class Frame(val index: Int, val size: Int, val pts: Long, val flags: Int, val arrivedNs: Long)

    private inner class Decoder(val width: Int, val height: Int, val fps: Int, val generation: Long) : Thread("kite-video") {
        val frames = ArrayBlockingQueue<Frame>(3)
        val ready = CountDownLatch(1)
        val stopping = AtomicBoolean(false)
        @Volatile var configured = false

        fun stopDecoder() { stopping.set(true); interrupt() }

        override fun run() {
            var codec:MediaCodec?=null
            var callbacks:HandlerThread?=null
            var pending:Frame?=null
            val budget=VideoFeedBudget()
            try {
                runCatching{Process.setThreadPriority(Process.THREAD_PRIORITY_DISPLAY)}
                val name=decoderName(width,height,fps)?:error("decoder size unsupported")
                val decoder=MediaCodec.createByCodecName(name);codec=decoder
                val format=MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC,width,height).apply {
                    setInteger(MediaFormat.KEY_MAX_INPUT_SIZE,8*1024*1024)
                    setInteger(MediaFormat.KEY_FRAME_RATE,fps)
                    setInteger(MediaFormat.KEY_PRIORITY,0)
                    setFloat(MediaFormat.KEY_OPERATING_RATE,fps.toFloat())
                    if(Build.VERSION.SDK_INT>=30)setInteger(MediaFormat.KEY_LOW_LATENCY,1)
                    if(Build.VERSION.SDK_INT>=31)setInteger(MediaFormat.KEY_ALLOW_FRAME_DROP,1)
                }
                decoder.configure(format,surface,null,0)
                decoder.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)
                val accelerated=if(Build.VERSION.SDK_INT>=29)decoder.codecInfo.isHardwareAccelerated else !name.startsWith("OMX.google.")&&!name.startsWith("c2.android.")
                performance.configured(name,accelerated)
                Log.i("KiteVideo","decoder=$name hardware=$accelerated size=${width}x${height} fps=$fps")
                val callbackThread=HandlerThread("kite-render-events").apply{start()};callbacks=callbackThread
                decoder.setOnFrameRenderedListener({_,pts,actual->
                    if(!stopping.get()&&!closed.get()){
                        performance.rendered(pts,actual)
                        NativeMedia.frameRendered(this@VideoRenderer.id,generation)
                    }
                },Handler(callbackThread.looper))
                decoder.start();configured=true;ready.countDown()
                val info=MediaCodec.BufferInfo()
                fun drain():Boolean {
                    var latest=-1;var work=false
                    for(i in 0 until 16){
                        val output=decoder.dequeueOutputBuffer(info,0)
                        if(output>=0){
                            if(latest>=0){decoder.releaseOutputBuffer(latest,false);performance.superseded()}
                            latest=output;work=true
                        }else if(output==MediaCodec.INFO_OUTPUT_FORMAT_CHANGED){decoder.setVideoScalingMode(MediaCodec.VIDEO_SCALING_MODE_SCALE_TO_FIT)}
                        else break
                    }
                    if(latest>=0)decoder.releaseOutputBuffer(latest,System.nanoTime())
                    return work
                }
                while(!stopping.get()&&!closed.get()){
                    drain()
                    if(pending==null)pending=frames.poll(2,TimeUnit.MILLISECONDS)
                    val frame=pending
                    if(frame!=null){
                        val input=decoder.dequeueInputBuffer(0)
                        if(input>=0){
                            try{
                                val target=decoder.getInputBuffer(input)?:error("decoder input unavailable");target.clear()
                                if(frame.size>target.remaining())error("decoder input too small")
                                val source=buffers[frame.index];source.position(0);source.limit(frame.size);target.put(source)
                                val flags=when {frame.flags and 1!=0->MediaCodec.BUFFER_FLAG_CODEC_CONFIG;frame.flags and 2!=0->MediaCodec.BUFFER_FLAG_KEY_FRAME;else->0}
                                decoder.queueInputBuffer(input,0,frame.size,frame.pts,flags)
                                if(frame.flags and 1==0)performance.submitted(frame.arrivedNs)
                            }finally{NativeMedia.releaseBuffer(this@VideoRenderer.id,frame.index);pending=null}
                        }else{
                            performance.inputWait()
                            if(budget.expired(frame.arrivedNs,System.nanoTime())){
                                NativeMedia.releaseBuffer(this@VideoRenderer.id,frame.index);pending=null;performance.inputTimeout()
                                if(budget.requestIdr(System.nanoTime())){performance.idr();NativeMedia.requestIdr(this@VideoRenderer.id)}
                            }else if(!drain())Thread.sleep(1)
                        }
                    }
                    drain()
                }
            }catch(_:InterruptedException){
                // Normal Surface/lifecycle shutdown.
            }catch(error:Exception){
                if(!stopping.get()&&!closed.get()){
                    Log.e("KiteVideo","decoder failed",error)
                    NativeMedia.decoderFailed(this@VideoRenderer.id,generation)
                }
            }finally{
                ready.countDown();stopping.set(true)
                pending?.let{NativeMedia.releaseBuffer(this@VideoRenderer.id,it.index)}
                while(true){val frame=frames.poll()?:break;NativeMedia.releaseBuffer(this@VideoRenderer.id,frame.index)}
                try{codec?.setOnFrameRenderedListener(null,null)}catch(_:Exception){}
                try{codec?.stop()}catch(_:Exception){}
                try{codec?.release()}catch(_:Exception){}
                callbacks?.quitSafely()
            }
        }
    }
}
