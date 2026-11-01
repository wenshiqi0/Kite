package io.github.wenshiqi0.kite

import android.app.NotificationManager
import android.content.Intent
import android.os.SystemClock
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.ConnectionStatus
import io.github.wenshiqi0.kite.data.AgentNotifications
import io.github.wenshiqi0.kite.ui.HostCheckViewModel
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** One explicitly enabled, owned, tool-free model turn. No system-push claim is made. */
class LiveAgentNotificationTest {
    @Test fun hostEventReachesAndroidAndTapReturnsToTheVerifiedConversation() {
        val test=InstrumentationRegistry.getInstrumentation();val args=InstrumentationRegistry.getArguments()
        assumeTrue(args.getString("qa_notifications")=="true")
        val cwd="/home/wenshiqi/.cache/kite-alpha28-notification-qa/project"
        val activity=test.startActivitySync(Intent(test.targetContext,MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) as MainActivity
        lateinit var vm:HostCheckViewModel
        fun main(action:()->Unit){test.runOnMainSync(action)}
        fun waitFor(label:String,predicate:()->Boolean){val deadline=SystemClock.uptimeMillis()+90_000;while(!predicate()&&SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100);assertTrue(label,predicate())}
        main{vm=ViewModelProvider(activity)[HostCheckViewModel::class.java]}
        waitFor("Initialized"){vm.engine.state.value?.initialized==true}
        val initial=vm.engine.state.value!!;val host=initial.hosts.single{it.id==initial.selectedHost}
        assertEquals("10.0.2.2",host.host);assertEquals(22223,host.port.toInt())
        if(initial.connection!=ConnectionStatus.CONNECTED)runBlocking{vm.engine.core.connect(host.id)}
        waitFor("Connected"){vm.engine.state.value?.connection==ConnectionStatus.CONNECTED}
        main{vm.settingsVisible=false;vm.closeDesktop()}
        val title="Kite Alpha 28 owned notification validation"
        main{vm.createSession("codex",cwd,title)}
        waitFor("Owned conversation created"){vm.engine.state.value?.sessions?.any{it.cwd==cwd&&it.title==title&&it.id==vm.engine.state.value?.selectedSession}==true}
        val session=vm.engine.state.value!!.selectedSession!!
        val file=File(test.targetContext.getExternalFilesDir(null),"live-agent-notification.json")
        val record=JSONObject().put("sessionId",session).put("cwd",cwd).put("owned",true)
        file.writeText(record.toString(2))
        waitFor("Composer ready"){vm.engine.state.value?.composer?.ready==true&&!vm.sessionBusy}
        val state=vm.engine.state.value!!;val agent=state.selectedAgent!!
        val model=state.composer.models.firstOrNull{it.model.contains("luna")||it.model.contains("mini")}?:state.composer.models.first()
        val effort=listOf("none","minimal","low","medium").firstOrNull{it in model.efforts}?:model.defaultEffort
        runBlocking{vm.engine.core.setChatModel(agent,model.model,effort)}
        // Let the independent host scanner establish its initial source checkpoint.
        SystemClock.sleep(2600)
        main{vm.engine.visible(false);vm.draft="Reply exactly KITE_NOTIFICATION_OK. Do not use tools or inspect files.";vm.sendPrompt(agent)}
        waitFor("Durable host completion event"){vm.engine.notificationInbox.value.any{it.sessionId==session}}
        val notice=vm.engine.notificationInbox.value.first{it.sessionId==session}
        val manager=test.targetContext.getSystemService(NotificationManager::class.java)
        waitFor("Actual Android notification"){manager.activeNotifications.any{it.tag==notice.id}}
        record.put("eventId",notice.id).put("nativePosted",true);file.writeText(record.toString(2))
        val posted=manager.activeNotifications.single{it.tag==notice.id}
        posted.notification.contentIntent.send()
        waitFor("Tap acknowledged after opening exact session"){vm.engine.notificationInbox.value.any{it.id==notice.id&&it.read}&&vm.engine.state.value?.selectedSession==session&&!vm.engine.notificationNavigationActive}
        record.put("warmTapVerified",true)
        runBlocking{vm.engine.core.disconnect()}
        main{vm.engine.visible(false);activity.moveTaskToBack(true)}
        // Repost the same verified event for an external cold-start tap after `am kill`.
        assertTrue(AgentNotifications(test.targetContext).deliver(notice))
        record.put("coldTapPrepared",true).put("status","PASS");file.writeText(record.toString(2))
    }
}
