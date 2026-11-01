package io.github.wenshiqi0.kite.media

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Handler
import android.os.Looper
import android.os.Process
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/** Opus and AudioTrack stay off the video, network and UI threads. */
class AudioRenderer(context:Context,private val rate:Int,private val channels:Int,private val frameSamples:Int,initialEnabled:Boolean) {
    data class Snapshot(val running:Boolean,val enabled:Boolean,val focused:Boolean,val rate:Int,val channels:Int,val packetSamples:Int,val packets:Long,val decodedFrames:Long,
        val writtenFrames:Long,val playedFrames:Long,val nonSilentFrames:Long,val droppedPackets:Long,val maxQueued:Int,val underruns:Int,val problem:String) {
        fun diagnostics()="audio running=$running enabled=$enabled focus=$focused format=$rate/$channels packet_samples=$packetSamples\npackets=$packets decoded/written/played=$decodedFrames/$writtenFrames/$playedFrames non_silent=$nonSilentFrames\naudio_drop=$droppedPackets queue_max=$maxQueued underruns=$underruns $problem"
    }
    private data class Packet(val bytes:ByteArray,val pts:Long,val arrived:Long)
    private val manager=context.applicationContext.getSystemService(AudioManager::class.java)
    private val packets=ArrayBlockingQueue<Packet>((rate/10/frameSamples).coerceIn(2,20))
    private val closed=AtomicBoolean(false)
    private val sequence=AtomicLong()
    private val received=AtomicLong()
    private val dropped=AtomicLong()
    @Volatile var enabled=initialEnabled
    @Volatile private var focused=false
    @Volatile private var requestFocus=true
    @Volatile private var running=false
    @Volatile private var decoded=0L
    @Volatile private var written=0L
    @Volatile private var played=0L
    @Volatile private var nonSilent=0L
    @Volatile private var maxQueued=0
    @Volatile private var underruns=0
    @Volatile private var problem=""
    private val attributes=AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build()
    private val focus=AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(attributes)
        .setOnAudioFocusChangeListener({value->if(!closed.get())focused=value==AudioManager.AUDIOFOCUS_GAIN},Handler(Looper.getMainLooper())).build()
    private val worker=Thread(::decode,"kite-audio").apply{start()}

    fun packet(bytes:ByteArray) {
        if(closed.get()||bytes.isEmpty()||bytes.size>4096)return
        received.incrementAndGet()
        val packet=Packet(bytes,sequence.getAndIncrement()*frameSamples*1_000_000L/rate,System.nanoTime())
        if(!packets.offer(packet)){packets.poll();dropped.incrementAndGet();if(!packets.offer(packet))dropped.incrementAndGet()}
        maxQueued=maxOf(maxQueued,packets.size)
    }
    fun snapshot()=Snapshot(running,enabled,focused,rate,channels,frameSamples,received.get(),decoded,written,played,nonSilent,dropped.get(),maxQueued,underruns,problem)
    fun close(){if(closed.compareAndSet(false,true))worker.interrupt()}

    private fun decode() {
        var codec:MediaCodec?=null;var track:AudioTrack?=null
        var pending:Packet?=null
        var pcmIndex=-1;var pcm:ByteBuffer?=null;var pcmBegan=0L
        var bytesPerSample=2;var playState=false;var lastEnabled=enabled;var lastHead=0L
        var lastAccount=0L
        fun account() {
            val current=track?:return
            val head=current.playbackHeadPosition.toLong() and 0xffffffffL
            played+=(head-lastHead) and 0xffffffffL;lastHead=head
            underruns=current.underrunCount
        }
        fun flushPlayer() {
            val current=track?:return
            account();current.pause();current.flush();lastHead=0;playState=false
        }
        try {
            runCatching{Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)}
            require(rate==48000&&channels==2&&frameSamples in 120..2880)
            codec=MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
            val decoder=codec
            val format=MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS,rate,channels).apply {
                setByteBuffer("csd-0",OpusStreamFormat.identification(rate,channels))
                setByteBuffer("csd-1",OpusStreamFormat.nativeLong(0))
                setByteBuffer("csd-2",OpusStreamFormat.nativeLong(80_000_000))
                setInteger(MediaFormat.KEY_PCM_ENCODING,AudioFormat.ENCODING_PCM_16BIT)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE,4096)
                setInteger(MediaFormat.KEY_PRIORITY,0)
                setFloat(MediaFormat.KEY_OPERATING_RATE,rate.toFloat())
            }
            decoder.configure(format,null,null,0);decoder.start();running=true
            val info=MediaCodec.BufferInfo()
            while(!closed.get()) {
                var worked=false
                if(enabled!=lastEnabled) {
                    lastEnabled=enabled;requestFocus=enabled
                    if(!enabled){focused=false;manager.abandonAudioFocusRequest(focus)}
                }
                if(enabled&&requestFocus) {
                    requestFocus=false
                    focused=manager.requestAudioFocus(focus)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED
                }
                val audible=enabled&&focused
                if(playState&&!audible)flushPlayer()
                for(batch in 0 until 8) {
                if(pcmIndex>=0) {
                    val data=requireNotNull(pcm)
                    if(!audible) {
                        decoder.releaseOutputBuffer(pcmIndex,false);pcmIndex=-1;pcm=null
                        worked=true
                    }else {
                        val current=requireNotNull(track)
                        if(!playState){current.play();playState=true}
                        val count=current.write(data,data.remaining(),AudioTrack.WRITE_NON_BLOCKING)
                        check(count>=0){"audio output error $count"};written+=count/(channels*bytesPerSample)
                        worked=worked||count>0
                        if(System.nanoTime()-lastAccount>=50_000_000L){account();lastAccount=System.nanoTime()}
                        if(!data.hasRemaining()||System.nanoTime()-pcmBegan>100_000_000L) {
                            if(data.hasRemaining())flushPlayer()
                            decoder.releaseOutputBuffer(pcmIndex,false);pcmIndex=-1;pcm=null
                        }else if(count==0)break
                    }
                }
                if(pcmIndex<0) {
                    val output=decoder.dequeueOutputBuffer(info,0)
                    if(output==MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                        worked=true
                        val next=decoder.outputFormat
                        val encoding=if(next.containsKey(MediaFormat.KEY_PCM_ENCODING))next.getInteger(MediaFormat.KEY_PCM_ENCODING)else AudioFormat.ENCODING_PCM_16BIT
                        require(next.getInteger(MediaFormat.KEY_SAMPLE_RATE)==rate&&next.getInteger(MediaFormat.KEY_CHANNEL_COUNT)==channels)
                        require(encoding==AudioFormat.ENCODING_PCM_16BIT||encoding==AudioFormat.ENCODING_PCM_FLOAT)
                        bytesPerSample=if(encoding==AudioFormat.ENCODING_PCM_FLOAT)4 else 2
                        track?.let{flushPlayer();it.release()};lastHead=0
                        val minimum=AudioTrack.getMinBufferSize(rate,AudioFormat.CHANNEL_OUT_STEREO,encoding);check(minimum>0)
                        track=AudioTrack.Builder().setAudioAttributes(attributes)
                            .setAudioFormat(AudioFormat.Builder().setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).setEncoding(encoding).build())
                            .setTransferMode(AudioTrack.MODE_STREAM).setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                            .setBufferSizeInBytes(maxOf(minimum,rate*channels*bytesPerSample/50)).build()
                        check(track.state==AudioTrack.STATE_INITIALIZED)
                    }else if(output>=0) {
                        worked=true
                        val data=requireNotNull(decoder.getOutputBuffer(output));data.position(info.offset);data.limit(info.offset+info.size)
                        decoded+=info.size/(channels*bytesPerSample)
                        val sample=data.duplicate().order(ByteOrder.nativeOrder());var signal=false
                        while(sample.remaining()>=bytesPerSample) {
                            if(if(bytesPerSample==2)kotlin.math.abs(sample.short.toInt())>8 else kotlin.math.abs(sample.float)>.00025f){signal=true;break}
                        }
                        if(signal)nonSilent+=info.size/(channels*bytesPerSample)
                        if(info.size==0||track==null){decoder.releaseOutputBuffer(output,false)}
                        else{pcmIndex=output;pcm=data;pcmBegan=System.nanoTime()}
                    }else break
                }
                }
                for(batch in 0 until 8) {
                if(pending==null)pending=packets.poll()
                val packet=pending
                if(packet!=null) {
                    if(System.nanoTime()-packet.arrived>100_000_000L){dropped.incrementAndGet();pending=null;worked=true}
                    else {
                        val input=decoder.dequeueInputBuffer(0)
                        if(input>=0) {
                            val buffer=requireNotNull(decoder.getInputBuffer(input));buffer.clear();buffer.put(packet.bytes)
                            decoder.queueInputBuffer(input,0,packet.bytes.size,packet.pts,0);pending=null
                            worked=true
                        }else break
                    }
                }else break
                }
                if(!worked)Thread.sleep(1)
            }
        }catch(_:InterruptedException) {
            // A normal stream/surface lifecycle transition.
        }catch(error:Exception) {
            problem=error.javaClass.simpleName+": "+error.message.orEmpty().take(160)
        }finally {
            running=false;closed.set(true);packets.clear();focused=false
            runCatching{manager.abandonAudioFocusRequest(focus)}
            runCatching{account();track?.pause();track?.flush();track?.release()}
            runCatching{codec?.stop()};runCatching{codec?.release()}
        }
    }
}
