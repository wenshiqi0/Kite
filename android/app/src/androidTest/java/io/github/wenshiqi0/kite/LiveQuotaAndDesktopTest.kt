package io.github.wenshiqi0.kite

import android.content.Intent
import android.graphics.Rect
import android.os.Build
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import android.view.InputDevice
import android.view.MotionEvent
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Explicit opt-in, owned session/desktop only. Never sends a model prompt. */
class LiveQuotaAndDesktopTest {
    @Test fun readOnlyQuotaStaysUsableAfterExplicitDesktopDisconnect() {
        val test=InstrumentationRegistry.getInstrumentation();val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_live_quota")=="true")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        test.runOnMainSync{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(label:String,condition:()->Boolean){val end=SystemClock.uptimeMillis()+60000;while(!condition()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(100);assertTrue(label,condition())}
        fun node(text:String?=null,description:String?=null):AccessibilityNodeInfo?{
            fun visit(n:AccessibilityNodeInfo?):AccessibilityNodeInfo?{
                n?:return null
                if(text!=null&&n.text?.toString()==text || description!=null&&n.contentDescription?.toString()==description)return n
                for(i in 0 until n.childCount)visit(n.getChild(i))?.let{return it}
                return null
            }
            if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
            return visit(test.uiAutomation.rootInActiveWindow)
        }
        fun tap(node:AccessibilityNodeInfo){
            val rect=Rect();node.getBoundsInScreen(rect);val time=SystemClock.uptimeMillis()
            for(action in listOf(MotionEvent.ACTION_DOWN,MotionEvent.ACTION_UP)){
                val event=MotionEvent.obtain(time,SystemClock.uptimeMillis(),action,rect.exactCenterX(),rect.exactCenterY(),0);event.source=InputDevice.SOURCE_TOUCHSCREEN
                try{assertTrue(test.uiAutomation.injectInputEvent(event,true))}finally{event.recycle()}
            }
            test.waitForIdleSync()
        }
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        if(args.getString("qa_phase")=="public-key"){
            File(test.targetContext.getExternalFilesDir(null),"quota-device-public-key.txt").writeText(requireNotNull(vm.engine.state.value?.deviceKey).publicKeyOpenssh)
            test.runOnMainSync{activity.finish()};return
        }
        val cwd=requireNotNull(args.getString("qa_cwd"));require(cwd.startsWith("/home/wenshiqi/.cache/kite-alpha20-qa-20261101/"))
        val before=requireNotNull(vm.engine.state.value);val host=before.hosts.single{it.id==before.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        if(before.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        test.runOnMainSync{vm.settingsVisible=false;vm.createSession("codex",cwd,"Kite Alpha 20 owned quota check")}
        waitFor("Owned chat ready"){
            val state=vm.engine.state.value
            state?.sessions?.any{it.id==state.selectedSession&&it.cwd==cwd}==true&&state.composer.ready
        }
        val selected=requireNotNull(vm.engine.state.value?.selectedSession)
        File(test.targetContext.getExternalFilesDir(null),"quota-owned-session.json").writeText("""{"sessionId":"$selected","cwd":"$cwd"}""")
        waitFor("Live quota read") { vm.engine.state.value?.composer?.usage?.let{it.status==UsageStatus.READY&&!it.loading}==true }
        val initial=requireNotNull(vm.engine.state.value).composer.usage
        assertNotNull(initial.plan);assertTrue(initial.buckets.isNotEmpty())
        for(bucket in initial.buckets)for(window in listOfNotNull(bucket.primary,bucket.secondary))assertTrue(window.remainingPercent<=100u)
        test.runOnMainSync{vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
        val pinFile=File(test.targetContext.getExternalFilesDir(null),"quota-pair-pin.txt");pinFile.delete()
        val requireVideo=args.getString("qa_video_required")=="true"
        if(requireVideo){
            val deadline=SystemClock.uptimeMillis()+60000
            while(vm.engine.state.value?.desktop?.status!=DesktopStatus.STREAMING&&SystemClock.uptimeMillis()<deadline){
                vm.engine.state.value?.desktop?.pin?.let{pinFile.writeText(it)};SystemClock.sleep(100)
            }
            waitFor("Owned desktop frames"){vm.engine.state.value?.desktop?.let{it.status==DesktopStatus.STREAMING&&it.renderedFrames>0uL}==true}
        }else{
            waitFor("Cancellable desktop setup"){vm.showDesktop&&vm.engine.state.value?.desktop?.status!=DesktopStatus.IDLE}
        }
        val videoFrames=vm.engine.state.value?.desktop?.renderedFrames?:0uL
        waitFor("Workspace menu"){node(description=test.targetContext.getString(R.string.more_actions))!=null}
        tap(requireNotNull(node(description=test.targetContext.getString(R.string.more_actions))))
        waitFor("Disconnect action"){node(text=test.targetContext.getString(R.string.desktop_disconnect))!=null}
        assertNull("Quota belongs in the model picker",node(text=test.targetContext.getString(R.string.usage_title)))
        tap(requireNotNull(node(text=test.targetContext.getString(R.string.desktop_disconnect))))
        waitFor("Stream disconnected"){!vm.showDesktop&&vm.engine.state.value?.desktop?.status==DesktopStatus.IDLE}
        val after=requireNotNull(vm.engine.state.value)
        assertEquals(ConnectionStatus.CONNECTED,after.connection);assertEquals(selected,after.selectedSession);assertTrue(after.composer.ready)
        val previousUpdate=after.composer.usage.updatedMs?:0
        test.runOnMainSync{vm.refreshUsage()}
        waitFor("Quota after stopping media"){vm.engine.state.value?.composer?.usage?.let{it.status==UsageStatus.READY&&!it.loading&&(it.updatedMs?:0)>previousUpdate}==true}
        tap(requireNotNull(node(description="模型与思考强度")))
        waitFor("Model picker quota"){node(text=test.targetContext.getString(R.string.usage_title))!=null}
        File(test.targetContext.getExternalFilesDir(null),"live-quota-desktop-validation.json").writeText("""{"status":"PASS","ownedSession":"$selected","modelPromptSubmitted":false,"planProvided":true,"quotaBuckets":${initial.buckets.size},"quotaInsideModelPicker":true,"noQuotaInWorkspaceMenu":true,"realVideoFrames":${videoFrames>0uL},"cancelDuringSetup":${!requireVideo},"explicitDisconnect":true,"chatConnectionRetained":true,"sameSessionRetained":true,"quotaReadAfterDisconnect":true}""")
        pinFile.delete();test.runOnMainSync{activity.finish()}
    }
}
