package io.github.wenshiqi0.kite.media

import java.util.ArrayDeque
import java.util.Locale

/** Bounded numeric samples only. No frame bytes, text, host identity or account data. */
class VideoPerformance(private val clock:()->Long={System.nanoTime()}) {
    data class Snapshot(val decoder:String,val hardware:Boolean,val receivedFps:Double,val submittedFps:Double,val renderedFps:Double,
        val received:Long,val submitted:Long,val rendered:Long,val queueP95Ms:Double,val renderP95Ms:Double,val callbackP95Ms:Double,
        val inputWaits:Long,val inputTimeouts:Long,val rejected:Long,val superseded:Long,val idrRequests:Long,val maxQueued:Int) {
        fun diagnostics()=String.format(Locale.ROOT,
            "decoder=%s hardware=%s\nreceive/submit/render_fps=%.1f/%.1f/%.1f\nqueue/render/callback_p95_ms=%.1f/%.1f/%.1f\ninput_waits=%d timeouts=%d rejected=%d superseded=%d idr=%d queue_max=%d",
            decoder,hardware,receivedFps,submittedFps,renderedFps,queueP95Ms,renderP95Ms,callbackP95Ms,inputWaits,inputTimeouts,rejected,superseded,idrRequests,maxQueued)
    }
    private class Samples {
        val values=LongArray(180);var count=0;var index=0
        fun add(value:Long){values[index]=value.coerceAtLeast(0);index=(index+1)%values.size;count=minOf(count+1,values.size)}
        fun p95():Double {if(count==0)return 0.0;val sorted=values.copyOf(count).sorted();return sorted[((count-1)*95+99)/100]/1_000_000.0}
    }
    private val began=clock();private var decoder="";private var hardware=false
    private val arrivals=ArrayDeque<Long>();private val submissions=ArrayDeque<Long>();private val renders=ArrayDeque<Long>()
    private val packetTimes=linkedMapOf<Long,Long>();private val queueAge=Samples();private val renderAge=Samples();private val callbackAge=Samples()
    private var received=0L;private var submitted=0L;private var rendered=0L
    private var inputWaits=0L;private var inputTimeouts=0L;private var rejected=0L;private var superseded=0L;private var idrs=0L;private var maxQueued=0
    @Synchronized fun configured(name:String,accelerated:Boolean){decoder=name;hardware=accelerated}
    @Synchronized fun received(pts:Long,queued:Int,at:Long){received++;arrivals.addLast(at);packetTimes[pts]=at;while(packetTimes.size>180)packetTimes.remove(packetTimes.keys.first());maxQueued=maxOf(maxQueued,queued);trim(arrivals,at)}
    @Synchronized fun submitted(arrived:Long){val now=clock();submitted++;submissions.addLast(now);queueAge.add(now-arrived);trim(submissions,now)}
    @Synchronized fun rendered(pts:Long,actual:Long){val now=clock();rendered++;val at=actual.takeIf{it>0&&it<=now}?:now;renders.addLast(at);trim(renders,now);packetTimes.remove(pts)?.let{renderAge.add(at-it)};callbackAge.add(now-at)}
    @Synchronized fun inputWait(){inputWaits++}
    @Synchronized fun inputTimeout(){inputTimeouts++}
    @Synchronized fun rejected(){rejected++}
    @Synchronized fun superseded(){superseded++}
    @Synchronized fun idr(){idrs++}
    private fun trim(values:ArrayDeque<Long>,now:Long){while(values.isNotEmpty()&&(values.first()<now-2_000_000_000L||values.size>180))values.removeFirst()}
    @Synchronized fun snapshot():Snapshot {
        val now=clock();for(values in listOf(arrivals,submissions,renders))trim(values,now)
        val seconds=minOf(2_000_000_000L,(now-began).coerceAtLeast(1)).toDouble()/1_000_000_000.0
        return Snapshot(decoder,hardware,arrivals.size/seconds,submissions.size/seconds,renders.size/seconds,received,submitted,rendered,queueAge.p95(),renderAge.p95(),callbackAge.p95(),inputWaits,inputTimeouts,rejected,superseded,idrs,maxQueued)
    }
}

/** Transient codec pressure must not immediately discard a reference picture. */
class VideoFeedBudget {
    private var lastIdr:Long?=null
    fun expired(arrivedNs:Long,nowNs:Long)=nowNs-arrivedNs>=50_000_000L
    fun requestIdr(nowNs:Long):Boolean {
        if(lastIdr?.let{nowNs-it<250_000_000L}==true)return false
        lastIdr=nowNs;return true
    }
}
