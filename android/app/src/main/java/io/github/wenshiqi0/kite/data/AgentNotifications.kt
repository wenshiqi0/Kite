package io.github.wenshiqi0.kite.data

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import io.github.wenshiqi0.kite.MainActivity
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.AgentNoticeInfo
import io.github.wenshiqi0.kite.core.AgentNoticeKind

/** System delivery and privacy only. Event identity/read state belong to Rust. */
class AgentNotifications(private val context:Context) {
    private val preferences=context.getSharedPreferences("agent-notifications",Context.MODE_PRIVATE)
    private val manager=context.getSystemService(NotificationManager::class.java)
    private val delivered=mutableSetOf<String>()
    @Volatile var enabled=preferences.getBoolean("enabled",true)
        private set
    fun enable(value:Boolean){enabled=value;preferences.edit().putBoolean("enabled",value).apply();if(!value){for(notice in manager.activeNotifications.filter{it.notification.channelId==ATTENTION||it.notification.channelId==COMPLETE})manager.cancel(notice.tag,notice.id);delivered.clear()}}
    fun permissionGranted()=manager.areNotificationsEnabled()&&(Build.VERSION.SDK_INT<33||ContextCompat.checkSelfPermission(context,Manifest.permission.POST_NOTIFICATIONS)==PackageManager.PERMISSION_GRANTED)
    fun shouldAskPermission()=enabled&&Build.VERSION.SDK_INT>=33&&!permissionGranted()&&!preferences.getBoolean("asked",false)
    fun permissionAsked(){preferences.edit().putBoolean("asked",true).apply()}
    init {
        manager.createNotificationChannel(NotificationChannel(ATTENTION,context.getString(R.string.notice_attention_channel),NotificationManager.IMPORTANCE_HIGH))
        manager.createNotificationChannel(NotificationChannel(COMPLETE,context.getString(R.string.notice_complete_channel),NotificationManager.IMPORTANCE_DEFAULT))
    }
    @SuppressLint("MissingPermission")
    @Synchronized fun deliver(notice:AgentNoticeInfo):Boolean {
        if(!enabled||!permissionGranted()||notice.id in delivered)return false
        val channel=if(notice.kind==AgentNoticeKind.COMPLETED)COMPLETE else ATTENTION
        val intent=Intent(context,MainActivity::class.java).setAction(OPEN)
            .setData(Uri.Builder().scheme("kite").authority("notification").appendPath(notice.id).build())
            .putExtra(ID,notice.id).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val open=PendingIntent.getActivity(context,notice.id.hashCode(),intent,PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val body=context.getString(when(notice.kind){AgentNoticeKind.COMPLETED->R.string.notice_completed;AgentNoticeKind.QUESTION->R.string.notice_question;AgentNoticeKind.ATTENTION->R.string.notice_attention;AgentNoticeKind.FAILED->R.string.notice_failed})
        val public=NotificationCompat.Builder(context,channel).setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("Kite").setContentText(context.getString(R.string.notice_private)).build()
        val notification=NotificationCompat.Builder(context,channel).setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(notice.title.ifBlank{context.getString(R.string.notice_title)}).setContentText(body)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(public)
            .setContentIntent(open).setAutoCancel(true).setOnlyAlertOnce(true).setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setGroup("kite-agent-${notice.hostId}").build()
        manager.notify(notice.id,2,notification);delivered.add(notice.id)
        if(delivered.size>400)delivered.retainAll(delivered.toList().takeLast(200).toSet())
        return true
    }
    @Synchronized fun cancel(id:String){manager.cancel(id,2);delivered.remove(id)}
    companion object {
        const val OPEN="io.github.wenshiqi0.kite.OPEN_AGENT_NOTIFICATION"
        const val ID="kite_notice_id"
        const val ATTENTION="kite-agent-attention"
        const val COMPLETE="kite-agent-complete"
        fun target(intent:Intent):String?=intent.getStringExtra(ID)?.takeIf{it.length<=300&&it.contains('/')}
    }
}
