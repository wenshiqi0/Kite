package io.github.wenshiqi0.kite.ui

import android.content.Intent
import android.provider.Settings
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AgentNoticeKind
import io.github.wenshiqi0.kite.data.SystemPush
import java.text.DateFormat
import java.util.Date

@Composable
fun AgentNotificationCenter(vm:HostCheckViewModel,onOpen:(String)->Unit,onClose:()->Unit) {
    val context=LocalContext.current
    val inbox by vm.engine.notificationInbox.collectAsState()
    val state by vm.engine.state.collectAsState()
    var enabled by remember{mutableStateOf(vm.engine.agentNotifications.enabled)}
    AlertDialog(onDismissRequest=onClose,title={Text(stringResource(R.string.notice_center))},text={
        Column(Modifier.fillMaxWidth().heightIn(max=520.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween){
                Text(stringResource(R.string.notice_enabled),modifier=Modifier.weight(1f).padding(top=12.dp))
                Switch(checked=enabled,onCheckedChange={enabled=it;vm.engine.enableAgentNotifications(it)})
            }
            if(enabled&&!vm.engine.agentNotifications.permissionGranted())Text(stringResource(R.string.notice_permission_missing),style=MaterialTheme.typography.bodySmall)
            TextButton(onClick={context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE,context.packageName))}){Text(stringResource(R.string.notice_system_settings))}
            Text(stringResource(when(vm.engine.systemPush.status){SystemPush.Status.READY->R.string.notice_push_ready;SystemPush.Status.NO_PLAY_SERVICES->R.string.notice_push_no_play;SystemPush.Status.REGISTERING->R.string.notice_push_registering;SystemPush.Status.HOST_UNCONFIGURED->R.string.notice_push_host_missing;SystemPush.Status.RETRY->R.string.notice_push_retry;SystemPush.Status.PERMISSION->R.string.notice_permission_missing;SystemPush.Status.DISABLED->R.string.notice_push_disabled;else->R.string.notice_push_unconfigured}),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(inbox.isEmpty())Text(stringResource(R.string.notice_empty),modifier=Modifier.padding(vertical=20.dp))
            LazyColumn(Modifier.weight(1f,false),verticalArrangement=Arrangement.spacedBy(8.dp)){
                items(inbox,key={it.id}){notice->
                    Surface(Modifier.fillMaxWidth().clickable{onOpen(notice.id)},shape=MaterialTheme.shapes.medium,color=MaterialTheme.colorScheme.surfaceContainerHigh){
                        Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(4.dp)){
                            Text(notice.title,style=MaterialTheme.typography.titleSmall,fontWeight=if(notice.read)FontWeight.Normal else FontWeight.SemiBold,maxLines=2)
                            Text(stringResource(when(notice.kind){AgentNoticeKind.COMPLETED->R.string.notice_completed;AgentNoticeKind.QUESTION->R.string.notice_question;AgentNoticeKind.ATTENTION->R.string.notice_attention;AgentNoticeKind.FAILED->R.string.notice_failed}),style=MaterialTheme.typography.bodySmall)
                            val host=state?.hosts?.firstOrNull{it.id==notice.hostId}?.host.orEmpty()
                            Text(host+" · "+DateFormat.getDateTimeInstance(DateFormat.SHORT,DateFormat.SHORT).format(Date(notice.createdMs)),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        }
    },confirmButton={TextButton(onClick=onClose){Text(stringResource(R.string.dismiss))}})
}
