package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.Build
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.ui.text
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Owned host/project only. Final mode sends one tool-free, bounded model turn. */
class NewSessionRegressionTest {
    @Test fun freshSessionFirstReplyAndReconnect() {
        val test=InstrumentationRegistry.getInstrumentation();val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_new_session")=="true")
        val baseline=args.getString("qa_baseline")=="true"
        val cwd=requireNotNull(args.getString("qa_cwd"))
        require(cwd=="/home/wenshiqi/.cache/kite-alpha24-qa-20261101/project")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        fun main(action:()->Unit){var problem:Throwable?=null;test.runOnMainSync{try{action()}catch(e:Throwable){problem=e}};problem?.let{throw it}}
        main{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(label:String,condition:()->Boolean){val until=SystemClock.uptimeMillis()+90000;while(!condition()&&SystemClock.uptimeMillis()<until)SystemClock.sleep(100);assertTrue(label,condition())}
        fun visibleText(text:String):Boolean {
            fun visit(node:AccessibilityNodeInfo?):Boolean {
                node?:return false
                if(node.text?.toString()==text)return true
                for(i in 0 until node.childCount)if(visit(node.getChild(i)))return true
                return false
            }
            if(Build.VERSION.SDK_INT>=33)test.uiAutomation.clearCache()
            return visit(test.uiAutomation.rootInActiveWindow)
        }
        fun waiting():Boolean=runCatching{val value=vm.engine.state.value!!.history;value.javaClass.getMethod("getWaiting").invoke(value) as Boolean}.getOrDefault(false)
        val output=test.targetContext.getExternalFilesDir(null)
        val owned=JSONArray();var terminalSeen=false;var falseFailureSeen=false
        fun saveOwned(){File(output,"new-session-owned.json").writeText(JSONObject().put("cwd",cwd).put("sessions",owned).toString(2))}
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        val initial=requireNotNull(vm.engine.state.value);val host=initial.hosts.single{it.id==initial.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt());assertEquals(48200,host.sunshinePort.toInt())
        if(initial.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        main{vm.settingsVisible=false;vm.closeDesktop()}
        fun fresh():String {
            waitFor("Previous session operation completed"){!vm.sessionBusy&&!vm.creatingAgent}
            val before=vm.engine.state.value!!.selectedSession
            main{vm.createSession("codex",cwd,"Kite Alpha 24 owned new-session regression")}
            waitFor("New session selected"){val s=vm.engine.state.value;s?.selectedSession!=null&&s.selectedSession!=before&&s.sessions.any{it.id==s.selectedSession&&it.cwd==cwd}}
            val id=requireNotNull(vm.engine.state.value?.selectedSession);owned.put(id);saveOwned()
            val until=SystemClock.uptimeMillis()+60000
            while(SystemClock.uptimeMillis()<until){
                val state=vm.engine.state.value!!
                falseFailureSeen=falseFailureSeen||state.history.failed
                terminalSeen=terminalSeen||visibleText(activity.getString(R.string.terminal_output))
                if(state.composer.ready)break
                if(state.composer.details.isNotBlank()&&!state.composer.loading)break
                SystemClock.sleep(120)
            }
            SystemClock.sleep(2200)
            falseFailureSeen=falseFailureSeen||vm.engine.state.value!!.history.failed
            terminalSeen=terminalSeen||visibleText(activity.getString(R.string.terminal_output))
            return id
        }
        try {
            val first=fresh();val before=vm.engine.state.value!!
            val base=JSONObject().put("version",activity.packageManager.getPackageInfo(activity.packageName,0).versionName)
                .put("baseline",baseline).put("composerReady",before.composer.ready).put("historyFailed",before.history.failed)
                .put("historyWaiting",waiting()).put("automaticTerminalSeen",terminalSeen)
                .put("composerDetails",before.composer.details.replace(Regex("[a-f0-9-]{36}"),"<thread>"))
            File(output,"new-session-before-send.json").writeText(base.toString(2))
            if(baseline)return
            assertTrue(before.composer.details,before.composer.ready)
            assertFalse("Fresh session was marked failed",falseFailureSeen)
            assertTrue("Fresh source should be pending",waiting())
            assertFalse("Startup terminal must not appear in Chat",terminalSeen)
            assertTrue(visibleText(activity.getString(R.string.new_session_ready)))
            val marker="KITE_NEW_SESSION_READY_24"
            main{vm.draft="Reply exactly $marker. Do not use tools or inspect files.";vm.sendPrompt(requireNotNull(before.selectedAgent))}
            waitFor("First assistant reply") {vm.engine.timeline.messages.any{it.role==ItemRole.ASSISTANT&&it.text.contains(marker)}}
            waitFor("History materialized") {vm.engine.state.value!!.history.complete&&!vm.engine.state.value!!.history.failed&&!waiting()}
            val firstCount=vm.engine.timeline.messages.size;assertTrue(firstCount>=2)
            runBlocking{vm.engine.core.disconnect();vm.engine.core.connect(host.id)}
            waitFor("Reconnected") {vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
            main{vm.settingsVisible=false;vm.openSession(first)}
            waitFor("Same conversation restored") {vm.engine.state.value?.selectedSession==first&&vm.engine.timeline.messages.any{it.role==ItemRole.ASSISTANT&&it.text.contains(marker)}&&vm.engine.state.value!!.composer.ready&&!vm.sessionBusy}
            val second=fresh();assertNotEquals(first,second)
            assertTrue(vm.engine.state.value!!.composer.ready);assertFalse(vm.engine.state.value!!.history.failed);assertTrue(waiting())
            assertFalse(vm.engine.timeline.messages.any{it.text.contains(marker)})
            assertFalse(terminalSeen);assertFalse(falseFailureSeen)
            val result=base.put("status","PASS").put("firstModelReply",true).put("promptCount",1)
                .put("sameSessionRestored",true).put("secondFreshSessionReady",true).put("crossSessionBleed",false)
                .put("automaticTerminalSeen",false).put("falseSyncFailureSeen",false).put("firstConversationMessages",firstCount)
            File(output,"new-session-validation.json").writeText(result.toString(2))
        } finally {main{activity.finish()}}
    }
}
