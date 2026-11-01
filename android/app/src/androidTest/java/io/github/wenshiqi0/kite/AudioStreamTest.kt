package io.github.wenshiqi0.kite

import android.content.Context
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.Intent
import android.graphics.Rect
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.media.VideoSurface
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Own Sunshine and private PulseAudio test-tone server only. No microphone capture. */
class AudioStreamTest {
    @Test fun opusPlaybackMuteFocusAndRepeatedDisconnect() {
        val test=InstrumentationRegistry.getInstrumentation();val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_audio_stream")=="true")
        test.uiAutomation.serviceInfo=test.uiAutomation.serviceInfo.apply{flags=flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS}
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel;var video:VideoSurface?=null
        fun main(action:()->Unit){var error:Throwable?=null;test.runOnMainSync{try{action()}catch(e:Throwable){error=e}};error?.let{throw it}}
        main{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun inspect(view:View):VideoSurface?{if(view is VideoSurface)return view;if(view is ViewGroup)for(i in 0 until view.childCount)inspect(view.getChildAt(i))?.let{return it};return null}
        val output=test.targetContext.getExternalFilesDir(null)
        fun waitFor(label:String,condition:()->Boolean) {
            val end=SystemClock.uptimeMillis()+60000
            while(!condition()&&SystemClock.uptimeMillis()<end){main{video=inspect(activity.window.decorView)};SystemClock.sleep(80)}
            if(!condition())File(output,"audio-stream-failure.txt").writeText("$label\n${video?.audioPerformance?.diagnostics()}\n${vm.engine.state.value?.failure?.details}")
            assertTrue("$label: ${video?.audioPerformance?.diagnostics()}",condition())
        }
        fun node(text:String?=null,description:String?=null):AccessibilityNodeInfo? {
            fun visit(n:AccessibilityNodeInfo?):AccessibilityNodeInfo?{n?:return null;if(text!=null&&n.text?.toString()==text||description!=null&&n.contentDescription?.toString()==description)return n;for(i in 0 until n.childCount)visit(n.getChild(i))?.let{return it};return null}
            if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
            visit(test.uiAutomation.rootInActiveWindow)?.let{return it}
            for(window in test.uiAutomation.windows)visit(window.root)?.let{return it}
            return null
        }
        fun tap(n:AccessibilityNodeInfo){
            val b=Rect();n.getBoundsInScreen(b);val began=SystemClock.uptimeMillis()
            for(action in listOf(MotionEvent.ACTION_DOWN,MotionEvent.ACTION_UP)){
                val event=MotionEvent.obtain(began,SystemClock.uptimeMillis(),action,b.exactCenterX(),b.exactCenterY(),0);event.source=InputDevice.SOURCE_TOUCHSCREEN
                try{assertTrue(test.uiAutomation.injectInputEvent(event,true))}finally{event.recycle()}
            }
        }
        fun audioMenu(label:Int){
            waitFor("Workspace menu"){node(description=activity.getString(R.string.more_actions))!=null}
            tap(requireNotNull(node(description=activity.getString(R.string.more_actions))))
            waitFor("Audio menu"){node(text=activity.getString(label))!=null}
            tap(requireNotNull(node(text=activity.getString(label))))
        }
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        val state=requireNotNull(vm.engine.state.value);val host=state.hosts.single{it.id==state.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        if(state.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        val initial=vm.engine.desktopAudioEnabled;val samples=JSONArray();val steady=JSONObject()
        val manager=activity.getSystemService(Context.AUDIO_SERVICE) as AudioManager
        val otherFocus=AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setOnAudioFocusChangeListener({}).build()
        try {
            main{vm.settingsVisible=false;vm.engine.setDesktopAudio(true);vm.closeDesktop()}
            repeat(3){index->
                main{vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
                waitFor("Actual Opus decode and playback on cycle $index") {
                    vm.engine.state.value?.desktop?.status==DesktopStatus.STREAMING&&video?.audioPerformance?.let{it.running&&it.focused&&it.playedFrames>=24000&&it.nonSilentFrames>=24000}==true
                }
                val audio=requireNotNull(video?.audioPerformance)
                assertEquals(48000,audio.rate);assertEquals(2,audio.channels);assertEquals(480,audio.packetSamples)
                assertTrue(audio.problem,audio.problem.isEmpty());assertTrue(audio.maxQueued<=20)
                val source=vm.engine.state.value!!.desktop;assertEquals(1920,source.width.toInt());assertEquals(1080,source.height.toInt())
                if(index==0){
                    SystemClock.sleep(1500)
                    val beforeSteady=requireNotNull(video?.audioPerformance);val began=SystemClock.elapsedRealtimeNanos()
                    SystemClock.sleep(4000)
                    val afterSteady=requireNotNull(video?.audioPerformance);val seconds=(SystemClock.elapsedRealtimeNanos()-began)/1e9
                    val received=afterSteady.packets-beforeSteady.packets;val dropped=afterSteady.droppedPackets-beforeSteady.droppedPackets
                    val fraction=(afterSteady.playedFrames-beforeSteady.playedFrames)/(48000.0*seconds)
                    steady.put("seconds",seconds).put("packets",received).put("dropped",dropped).put("dropRatio",dropped.toDouble()/received)
                        .put("playedFrames",afterSteady.playedFrames-beforeSteady.playedFrames).put("playedFraction",fraction).put("underruns",afterSteady.underruns-beforeSteady.underruns)
                    File(output,"audio-steady-validation.json").writeText(steady.toString(2))
                    assertTrue("Steady playback throughput: $steady",fraction>=.95)
                    assertTrue("Steady audio packet loss: $steady",received>0&&dropped.toDouble()/received<=.02)
                    audioMenu(R.string.desktop_audio_mute)
                    waitFor("Audio muted"){!vm.engine.desktopAudioEnabled&&video?.audioPerformance?.let{!it.enabled&&!it.focused}==true}
                    SystemClock.sleep(150);val muted=requireNotNull(video?.audioPerformance).playedFrames
                    SystemClock.sleep(250);assertEquals(muted,requireNotNull(video?.audioPerformance).playedFrames)
                    audioMenu(R.string.desktop_audio_enable)
                    waitFor("Audio resumes from current packets"){video?.audioPerformance?.let{it.focused&&it.enabled&&it.playedFrames>muted+9600}==true}
                    main{assertEquals(AudioManager.AUDIOFOCUS_REQUEST_GRANTED,manager.requestAudioFocus(otherFocus))}
                    waitFor("Transient focus loss silences remote audio"){video?.audioPerformance?.focused==false}
                    main{manager.abandonAudioFocusRequest(otherFocus)}
                    val afterFocus=requireNotNull(video?.audioPerformance).playedFrames
                    waitFor("Focus returns"){video?.audioPerformance?.let{it.focused&&it.playedFrames>afterFocus+9600}==true}
                }
                val final=requireNotNull(video?.audioPerformance)
                samples.put(JSONObject().put("rate",final.rate).put("channels",final.channels).put("packetSamples",final.packetSamples).put("packets",final.packets).put("decodedFrames",final.decodedFrames).put("writtenFrames",final.writtenFrames)
                    .put("playedFrames",final.playedFrames).put("nonSilentFrames",final.nonSilentFrames).put("queueMax",final.maxQueued).put("droppedPackets",final.droppedPackets).put("underruns",final.underruns))
                main{vm.closeDesktop()}
                waitFor("Audio resources released") {vm.engine.state.value?.desktop?.status==DesktopStatus.IDLE&&Thread.getAllStackTraces().keys.none{it.isAlive&&it.name=="kite-audio"}}
                assertEquals(ConnectionStatus.CONNECTED,vm.engine.state.value?.connection)
            }
            File(output,"audio-stream-validation.json").writeText(JSONObject().put("status","PASS").put("cycles",samples).put("steady",steady)
                .put("muteAndResume",true).put("focusLossAndReturn",true).put("audioThreadsReleased",true).put("realDecodedSignal",true)
                .put("physicalSpeakerAudibilityVerified",false).put("productionAudioRouteTouched",false).toString(2))
        }finally{main{manager.abandonAudioFocusRequest(otherFocus);vm.closeDesktop();vm.engine.setDesktopAudio(initial);activity.finish()}}
    }
}
