package io.github.wenshiqi0.kite

import android.content.Intent
import android.accessibilityservice.AccessibilityServiceInfo
import android.os.SystemClock
import android.view.View
import android.view.MotionEvent
import android.view.InputDevice
import android.graphics.Rect
import android.view.accessibility.AccessibilityNodeInfo
import android.view.ViewGroup
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.DesktopStatus
import io.github.wenshiqi0.kite.ui.DesktopLayout
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.ui.MarkdownTextView
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Explicitly owned fixture only. Checks real list allocation rather than a small screenshot. */
class LongTranscriptPresentationTest {
    @Test fun fullDataHasBoundedViewsAndVideoTelemetryDoesNotReplaceMessages() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_live")=="true")
        val session=requireNotNull(args.getString("qa_session"));require(session.startsWith("kite-qa-history-"))
        val expectedRecords=args.getString("qa_records")?.toInt()?:10000
        require(expectedRecords in 10000..10010)
        val test=InstrumentationRegistry.getInstrumentation()
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        test.runOnMainSync{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(condition:()->Boolean) {
            val deadline=SystemClock.uptimeMillis()+30000
            while(!condition()&&SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100)
            assertTrue("Owned transcript did not become ready",condition())
        }
        waitFor{vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        val state=requireNotNull(vm.engine.state.value)
        val host=state.hosts.single{it.id==state.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        test.runOnMainSync{vm.openSession(session)}
        waitFor{vm.engine.state.value?.selectedSession==session&&vm.engine.timeline.messages.size==expectedRecords&&vm.engine.state.value?.history?.complete==true}
        assertEquals(expectedRecords,vm.engine.timeline.rows.size)
        test.runOnMainSync{vm.openDesktop();vm.changeLayout(DesktopLayout.Split)}
        waitFor{vm.engine.state.value?.desktop?.status==DesktopStatus.STREAMING&&vm.engine.state.value!!.desktop.renderedFrames>0uL}
        SystemClock.sleep(400)
        val revision=vm.engine.timeline.revision
        val first=vm.engine.timeline.messages.first()
        val middle=vm.engine.timeline.messages[5000]
        val last=vm.engine.timeline.messages.last()
        val before=vm.engine.state.value!!.desktop.renderedFrames
        waitFor{vm.engine.state.value!!.desktop.renderedFrames>before}
        assertEquals(revision,vm.engine.timeline.revision)
        assertSame(first,vm.engine.timeline.messages.first())
        assertSame(middle,vm.engine.timeline.messages[5000])
        assertSame(last,vm.engine.timeline.messages.last())
        // Inspect a normal reading position as well as the live tail.
        val nodes=mutableListOf<AccessibilityNodeInfo>()
        fun find(node:AccessibilityNodeInfo){
            if(node.contentDescription?.toString()=="聊天记录")nodes.add(node)
            for(i in 0 until node.childCount)node.getChild(i)?.let(::find)
        }
        val automation=test.uiAutomation
        val info=automation.serviceInfo
        info.flags=info.flags or AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS
        automation.serviceInfo=info
        var root=automation.rootInActiveWindow
        val rootDeadline=SystemClock.uptimeMillis()+5000
        while(root==null&&SystemClock.uptimeMillis()<rootDeadline){SystemClock.sleep(50);root=automation.rootInActiveWindow}
        val active=requireNotNull(root)
        assertEquals("io.github.wenshiqi0.kite",active.packageName?.toString())
        find(active)
        val bounds=Rect();nodes.single().getBoundsInScreen(bounds)
        repeat(5){
            val down=SystemClock.uptimeMillis()
            fun send(action:Int,p:Float){
                val event=MotionEvent.obtain(down,SystemClock.uptimeMillis(),action,bounds.exactCenterX(),bounds.top+bounds.height()*(.15f+.7f*p),0)
                event.source=InputDevice.SOURCE_TOUCHSCREEN
                try{assertTrue(test.uiAutomation.injectInputEvent(event,true))}finally{event.recycle()}
            }
            send(MotionEvent.ACTION_DOWN,0f)
            for(i in 1..8){SystemClock.sleep(8);send(MotionEvent.ACTION_MOVE,i/8f)}
            send(MotionEvent.ACTION_UP,1f);SystemClock.sleep(120)
        }
        SystemClock.sleep(300)
        var nativeViews=0
        test.runOnMainSync{
            fun visit(v:View){if(v is MarkdownTextView)nativeViews++;if(v is ViewGroup)for(i in 0 until v.childCount)visit(v.getChildAt(i))}
            visit(activity.window.decorView)
        }
        assertTrue("No rich message views were created",nativeViews>0)
        assertTrue("Long data must not create every message view: "+nativeViews,nativeViews<=32)
        File(test.targetContext.filesDir,"long-transcript-validation.json").writeText(
            """{"records":""" + expectedRecords + ""","nativeMarkdownViews":""" + nativeViews + ""","telemetryRetainsMessageObjects":true}"""
        )
    }
}
