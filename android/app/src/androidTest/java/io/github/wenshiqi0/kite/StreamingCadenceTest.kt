package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.SystemClock
import android.view.Choreographer
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.media.VideoSurface
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.json.JSONObject
import java.io.File

/** Sustained actual video, isolated Sunshine/X11 scene; does not send model prompts. */
class StreamingCadenceTest {
    @Test fun sustainedFramesAndMainThreadCadence() {
        val args=InstrumentationRegistry.getArguments();assumeTrue(args.getString("qa_stream_perf")=="true")
        val test=InstrumentationRegistry.getInstrumentation()
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        test.runOnMainSync{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(label:String,condition:()->Boolean){val end=SystemClock.uptimeMillis()+60000;while(!condition()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(50);assertTrue(label,condition())}
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        val state=requireNotNull(vm.engine.state.value);val host=state.hosts.single{it.id==state.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        if(state.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        test.runOnMainSync{vm.settingsVisible=false;vm.closeDesktop()}
        SystemClock.sleep(300)
        test.runOnMainSync{vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
        val pin=File(test.targetContext.getExternalFilesDir(null),"stream-perf-pin.txt");pin.delete()
        val deadline=SystemClock.uptimeMillis()+60000
        while(vm.engine.state.value?.desktop?.status!=DesktopStatus.STREAMING&&SystemClock.uptimeMillis()<deadline){
            val current=vm.engine.state.value
            if(current?.desktop?.status==DesktopStatus.FAILED){
                val details="stage=${current.desktop.stage} ${current.failure?.details}"
                File(test.targetContext.getExternalFilesDir(null),"stream-perf-failure.txt").writeText(details)
                error(details)
            }
            current?.desktop?.pin?.let{pin.writeText(it)};SystemClock.sleep(100)
        }
        waitFor("Actual decoded frames"){vm.engine.state.value?.desktop?.let{it.status==DesktopStatus.STREAMING&&it.renderedFrames>=30uL}==true}
        pin.delete();SystemClock.sleep(2000)
        val frameIntervals=mutableListOf<Long>();var recording=true;var previous=0L
        val callback=object:Choreographer.FrameCallback{
            override fun doFrame(at:Long){if(!recording)return;if(previous!=0L)frameIntervals.add(at-previous);previous=at;Choreographer.getInstance().postFrameCallback(this)}
        }
        test.runOnMainSync{Choreographer.getInstance().postFrameCallback(callback)}
        var old=vm.engine.state.value!!.desktop.renderedFrames
        waitFor("Fresh stats sample"){vm.engine.state.value!!.desktop.renderedFrames!=old}
        val start=SystemClock.elapsedRealtimeNanos();val first=vm.engine.state.value!!.desktop
        val duration=args.getString("qa_seconds")?.toLong()?:12L;require(duration in 10..30)
        while(SystemClock.elapsedRealtimeNanos()-start<duration*1_000_000_000L){SystemClock.sleep(50);assertEquals(DesktopStatus.STREAMING,vm.engine.state.value?.desktop?.status)}
        old=vm.engine.state.value!!.desktop.renderedFrames;waitFor("Final fresh stats sample"){vm.engine.state.value!!.desktop.renderedFrames!=old}
        val seconds=(SystemClock.elapsedRealtimeNanos()-start)/1_000_000_000.0;val last=vm.engine.state.value!!.desktop
        test.runOnMainSync{recording=false;Choreographer.getInstance().removeFrameCallback(callback)}
        val sorted=frameIntervals.sorted();assertTrue(sorted.isNotEmpty())
        var metrics:String?=null
        test.runOnMainSync{
            fun inspect(view:View){
                if(view.javaClass.name.endsWith("VideoSurface")){
                    // Reflection keeps this cadence harness runnable against the previous public APK.
                    runCatching{val value=view.javaClass.getMethod("getPerformance").invoke(view);metrics=value?.javaClass?.getMethod("diagnostics")?.invoke(value)?.toString()}
                }
                if(view is ViewGroup)for(i in 0 until view.childCount)inspect(view.getChildAt(i))
            }
            inspect(activity.window.decorView)
        }
        val result=JSONObject().put("status","PASS").put("modelPromptSubmitted",false)
            .put("sourceWidth",last.sourceWidth.toLong()).put("sourceHeight",last.sourceHeight.toLong())
            .put("width",last.width.toLong()).put("height",last.height.toLong()).put("targetFps",last.fps.toLong())
            .put("seconds",seconds).put("receivedFps",(last.receivedFrames-first.receivedFrames).toDouble()/seconds)
            .put("renderedFps",(last.renderedFrames-first.renderedFrames).toDouble()/seconds)
            .put("uiFrameP95Ms",sorted[((sorted.size-1)*95+99)/100]/1_000_000.0)
            .put("uiFrameMaxMs",sorted.last()/1_000_000.0).put("decoderMetrics",metrics?:JSONObject.NULL)
        File(test.targetContext.getExternalFilesDir(null),"stream-cadence-validation.json").writeText(result.toString(2))
        assertTrue("No sustained video output",last.renderedFrames-first.renderedFrames>0uL)

        // Exercise the actual View -> ordered input sampler -> JNI -> Sunshine path.
        // The owned scene records motion count/span independently on the host.
        test.runOnMainSync{vm.changeLayout(DesktopLayout.Fullscreen);vm.takeOver(true)}
        waitFor("Operate mode"){vm.engine.state.value?.desktop?.takeover==true}
        SystemClock.sleep(800)
        lateinit var video:VideoSurface
        test.runOnMainSync{
            fun find(view:View):VideoSurface? {
                if(view is VideoSurface)return view
                if(view is ViewGroup)for(i in 0 until view.childCount)find(view.getChildAt(i))?.let{return it}
                return null
            }
            video=requireNotNull(find(activity.window.decorView))
        }
        val began=SystemClock.uptimeMillis()
        repeat(60){index->
            test.runOnMainSync{
                val x=video.width*(.25f+.5f*index/59f)
                val y=(video.height-176*activity.resources.displayMetrics.density)*.5f
                val event=MotionEvent.obtain(began,SystemClock.uptimeMillis(),MotionEvent.ACTION_HOVER_MOVE,x,y,0)
                event.source=InputDevice.SOURCE_MOUSE
                try{assertTrue(video.onHoverEvent(event))}finally{event.recycle()}
            }
            SystemClock.sleep(20)
        }
        val elapsed=SystemClock.uptimeMillis()-began
        SystemClock.sleep(400)
        File(test.targetContext.getExternalFilesDir(null),"continuous-pointer-validation.json").writeText(
            JSONObject().put("samples",60).put("gestureMs",elapsed).put("layout","fullscreen")
                .put("path","VideoSurface hover -> DesktopInput -> JNI -> owned Sunshine").toString(2))
        test.runOnMainSync{vm.closeDesktop();activity.finish()}
    }
}
