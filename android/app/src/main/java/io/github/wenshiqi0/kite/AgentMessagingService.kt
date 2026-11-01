package io.github.wenshiqi0.kite

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import io.github.wenshiqi0.kite.core.AgentNoticeKind
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.launch

/** FCM can create this service after ordinary process reclamation, without an SSH connection. */
class AgentMessagingService:FirebaseMessagingService() {
    override fun onRegistered(token:String){
        val engine=(application as KiteApplication).engine
        engine.scope.launch{engine.systemPush.tokenChanged(token)}
    }
    override fun onMessageReceived(message:RemoteMessage) {
        val engine=(application as KiteApplication).engine
        if(!engine.agentNotifications.enabled)return
        val id=message.data["kite_notice_id"]?:return
        val kind=when(message.data["kite_kind"]){"completed"->AgentNoticeKind.COMPLETED;"attention"->AgentNoticeKind.ATTENTION;"question"->AgentNoticeKind.QUESTION;"failed"->AgentNoticeKind.FAILED;else->return}
        val session=message.data["kite_session_id"]?:return
        val created=message.data["kite_created_ms"]?.toLongOrNull()?:return
        // Finish persistence and notification posting within the SDK callback's budget.
        // A push is only a hint; Core revalidates the event/session over pinned SSH on tap.
        runCatching{runBlocking(Dispatchers.IO){withTimeout(8_000){
            engine.state.filterNotNull().first{it.initialized}
            val notice=engine.core.receivePush(id,kind,session,created)
            engine.presentNotice(notice)
        }}}
    }
}
