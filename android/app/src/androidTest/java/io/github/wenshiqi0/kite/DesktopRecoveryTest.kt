package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.Build
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Isolated Sunshine only. A controller stops the owned server after the ready marker. */
class DesktopRecoveryTest {
    @Test fun repeatedStreamsAndAnInterruptedHostHaveExplicitStates() {
        val test=InstrumentationRegistry.getInstrumentation();val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_desktop_recovery")=="true")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        fun main(action:()->Unit){var error:Throwable?=null;test.runOnMainSync{try{action()}catch(e:Throwable){error=e}};error?.let{throw it}}
        main{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        val output=test.targetContext.getExternalFilesDir(null);val pin=File(output,"desktop-recovery-pin.txt");pin.delete()
        val interrupt=File(output,"desktop-recovery-interrupt-ready.txt");interrupt.delete()
        fun waitFor(label:String,condition:()->Boolean){
            val end=SystemClock.uptimeMillis()+60000
            while(!condition()&&SystemClock.uptimeMillis()<end){vm.engine.state.value?.desktop?.pin?.let{pin.writeText(it)};SystemClock.sleep(80)}
            assertTrue(label,condition())
        }
        fun visibleText(text:String):Boolean {
            fun visit(node:AccessibilityNodeInfo?):Boolean{node?:return false;if(node.text?.toString()==text)return true;for(i in 0 until node.childCount)if(visit(node.getChild(i)))return true;return false}
            if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
            return visit(test.uiAutomation.rootInActiveWindow)
        }
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        val initial=requireNotNull(vm.engine.state.value);val host=initial.hosts.single{it.id==initial.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        if(initial.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        val samples=JSONArray()
        try {
            main{vm.settingsVisible=false;vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
            repeat(3){index->
                if(index>0)main{vm.retryDesktop()}
                waitFor("Actual video on cycle $index") {vm.engine.state.value?.desktop?.let{it.status==DesktopStatus.STREAMING&&it.renderedFrames>=10uL}==true}
                val video=vm.engine.state.value!!.desktop
                assertEquals(1920,video.sourceWidth.toInt());assertEquals(1080,video.sourceHeight.toInt())
                samples.put(JSONObject().put("renderedFrames",video.renderedFrames.toLong()).put("width",video.width.toLong()).put("height",video.height.toLong()))
                runBlocking{vm.engine.core.stopDesktop()}
                waitFor("Idle remains usable") {vm.engine.state.value?.desktop?.status==DesktopStatus.IDLE&&visibleText(activity.getString(R.string.desktop_disconnected))}
                assertTrue(vm.showDesktop);assertFalse(visibleText(activity.getString(R.string.desktop_connecting)))
                assertEquals(ConnectionStatus.CONNECTED,vm.engine.state.value?.connection)
            }
            main{vm.retryDesktop()}
            waitFor("Video before interruption") {vm.engine.state.value?.desktop?.let{it.status==DesktopStatus.STREAMING&&it.renderedFrames>=10uL}==true}
            interrupt.writeText("owned Sunshine on base port 48200 only")
            waitFor("Interrupted stream leaves spinner state") {vm.engine.state.value?.desktop?.status==DesktopStatus.FAILED&&visibleText(activity.getString(R.string.desktop_failed))}
            assertTrue(visibleText(activity.getString(R.string.desktop_failed)))
            assertEquals(ConnectionStatus.CONNECTED,vm.engine.state.value?.connection)
            File(output,"desktop-recovery-validation.json").writeText(JSONObject().put("status","PASS").put("cycles",samples)
                .put("idleHasReconnectAction",true).put("interruptedStreamFailedExplicitly",true).put("sshRetained",true)
                .put("modelPromptSubmitted",false).put("productionDesktopTouched",false).toString(2))
        } finally {pin.delete();interrupt.delete();main{vm.closeDesktop();activity.finish()}}
    }
}
