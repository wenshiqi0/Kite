package io.github.wenshiqi0.kite

import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.core.HostRecord
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import io.github.wenshiqi0.kite.ui.text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Whole native client -> pinned QA SSH peers -> real Node -> native SQLite history. */
class NativeHistoryNavigationTest {
    @Test fun twoHostsLoadNativeHistoryAndSwitchWithoutWaitingForRemoteCatalogs() {
        val test=InstrumentationRegistry.getInstrumentation()
        val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_native_history_end_to_end")=="true")
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        test.runOnMainSync {vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        fun waitFor(label:String,timeout:Long=15000,condition:()->Boolean) {
            val deadline=SystemClock.uptimeMillis()+timeout
            while(!condition()&&SystemClock.uptimeMillis()<deadline)SystemClock.sleep(20)
            assertTrue(label+": "+vm.engine.state.value?.failure?.details.orEmpty()+" "+vm.engine.platformError.value.orEmpty(),condition())
        }
        waitFor("Core initialized"){vm.engine.state.value?.initialized==true}
        val directory=requireNotNull(test.targetContext.getExternalFilesDir(null))
        if(args.getString("qa_phase")=="public-key") {
            require(vm.engine.state.value!!.hosts.isEmpty())
            File(directory,"native-history-device.pub").writeText(vm.engine.state.value!!.deviceKey!!.publicKeyOpenssh)
            activity.finish();return
        }
        require(vm.engine.state.value!!.hosts.all {it.id.startsWith("owned-native-")})
        val metadata=JSONArray(File(directory,"native-history-hosts.json").readText())
        val ids=mutableListOf<Pair<String,String>>()
        for(index in 0 until metadata.length()) {
            val m=metadata.getJSONObject(index);val label=m.getString("label");val id="owned-native-$label"
            require(m.getString("host")=="10.0.2.2" && m.getInt("port") in 54000..54999)
            val host=HostRecord(id,"10.0.2.2",m.getInt("port").toUShort(),"owned",m.getString("herdr"),null,47989u)
            runBlocking(Dispatchers.Default){vm.engine.core.saveHost(host);vm.engine.core.connect(id)}
            waitFor("QA host reaches trust or connected") {vm.engine.state.value?.connection in listOf(ConnectionStatus.AWAITING_TRUST,ConnectionStatus.CONNECTED)}
            if(vm.engine.state.value!!.connection==ConnectionStatus.AWAITING_TRUST) {
                val key=requireNotNull(vm.engine.state.value!!.pendingHostKey)
                assertEquals(m.getString("fingerprint"),key.fingerprint)
                runBlocking(Dispatchers.Default){vm.engine.core.trustHost(id,key.fingerprint)}
            }
            waitFor("QA host connected"){vm.engine.state.value?.selectedHost==id&&vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
            waitFor("Saved QA sessions indexed"){vm.engine.state.value?.sessions?.size==2}
            val sessions=m.getJSONArray("sessions")
            for(j in 0 until sessions.length())ids+=id to sessions.getString(j)
        }
        test.runOnMainSync{vm.settingsVisible=false;vm.hostEditorVisible=false}
        val timings=JSONArray()
        for((host,session) in ids+ids.reversed()) {
            val start=SystemClock.uptimeMillis()
            test.runOnMainSync {vm.openSession(session,host)}
            waitFor("Selected session returns without remote history/catalog wait",2000) {
                vm.engine.state.value?.selectedHost==host&&vm.engine.state.value?.selectedSession==session&&!vm.sessionBusy
            }
            val selectedMs=SystemClock.uptimeMillis()-start
            assertTrue("Selection blocked on network: $selectedMs ms",selectedMs<750)
            val marker="NATIVE_HISTORY_FINAL_$session"
            waitFor("Native paginated substantive reply arrives",30000){vm.engine.timeline.messages.any {it.text==marker}}
            waitFor("Native reply is actually drawn",10000){
                fun find(node:AccessibilityNodeInfo?):Boolean {
                    node?:return false
                    if(node.text?.toString()==marker&&node.isVisibleToUser)return true
                    for(i in 0 until node.childCount)if(find(node.getChild(i)))return true
                    return false
                }
                test.uiAutomation.clearCache();find(test.uiAutomation.rootInActiveWindow)
            }
            val pulse=CountDownLatch(1);val posted=SystemClock.uptimeMillis();Handler(Looper.getMainLooper()).post{pulse.countDown()}
            assertTrue("UI thread stopped responding",pulse.await(750,TimeUnit.MILLISECONDS))
            timings.put(JSONObject().put("host",host).put("session",session).put("selectionMs",selectedMs).put("uiPulseMs",SystemClock.uptimeMillis()-posted).put("messageCount",vm.engine.timeline.messages.size))
        }
        File(directory,"native-history-navigation.json").writeText(JSONObject().put("status","PASS").put("cases",timings).toString(2))
        val screenshot=test.uiAutomation.takeScreenshot()
        File(directory,"native-history-navigation.png").outputStream().use{screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
        screenshot.recycle()
    }
}
