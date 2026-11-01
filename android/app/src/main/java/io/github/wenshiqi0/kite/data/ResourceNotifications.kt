package io.github.wenshiqi0.kite.data

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import io.github.wenshiqi0.kite.MainActivity
import io.github.wenshiqi0.kite.R
import io.github.wenshiqi0.kite.core.*
import io.github.wenshiqi0.kite.ui.resourceBytes

/** OS delivery only; memory pressure and admission decisions come from Rust. */
class ResourceNotifications(private val context: Context) {
    private val preferences = context.getSharedPreferences("host-resource-alerts", Context.MODE_PRIVATE)
    private val manager = context.getSystemService(NotificationManager::class.java)
    private val observed = mutableMapOf<String, String>()
    private val healthy = mutableSetOf<String>()
    init { manager.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.resource_alert_channel), NotificationManager.IMPORTANCE_HIGH)) }

    @SuppressLint("MissingPermission")
    fun observe(state: CoreSnapshot, visible: Boolean) {
        val host = state.selectedHost ?: return
        val profile = state.hostResources ?: return
        if (state.connection != ConnectionStatus.CONNECTED || !profile.available) return
        val level = when (profile.memoryPressure) { HostMemoryPressure.CRITICAL -> 2; HostMemoryPressure.WARNING -> 1; else -> 0 }
        if (level == 0) {
            val hadWarning = observed.remove(host) != null
            if (profile.memoryPressure == HostMemoryPressure.NORMAL && (healthy.add(host) || hadWarning)) manager.cancel("resource-$host", 3)
            return
        }
        healthy.remove(host)
        val key = "$level/${profile.reason}/${profile.agentOomKills}"
        if (observed[host] == key) return
        observed[host] = key
        if (visible || !manager.areNotificationsEnabled() || Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val now = System.currentTimeMillis()
        if (now - preferences.getLong("$host/time", 0) < 600_000 && level <= preferences.getInt("$host/level", 0)) return
        val intent = Intent(context, MainActivity::class.java).putExtra(HOST, host)
            .setData(Uri.Builder().scheme("kite").authority("resources").appendPath(host).build())
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val open = PendingIntent.getActivity(context, host.hashCode(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val title = context.getString(when (profile.reason) {
            MemoryWarningReason.HOST_MEMORY, MemoryWarningReason.MEMORY_STALL -> R.string.resource_host_memory_warning
            MemoryWarningReason.RECENT_OOM -> R.string.resource_oom_warning
            else -> R.string.resource_agent_memory_warning
        })
        val body = context.getString(R.string.resource_alert_body, resourceBytes(profile.memoryAvailableBytes), resourceBytes(profile.agentMemoryUsedBytes), resourceBytes(profile.agentMemoryLimitBytes))
        val public = NotificationCompat.Builder(context, CHANNEL).setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("Kite").setContentText(context.getString(R.string.resource_alert_channel)).build()
        manager.notify("resource-$host", 3, NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_launcher_foreground).setContentTitle(title).setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body)).setContentIntent(open).setAutoCancel(true)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(public).setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS).build())
        preferences.edit().putLong("$host/time", now).putInt("$host/level", level).apply()
    }

    companion object {
        const val HOST = "kite_resource_host"
        const val CHANNEL = "kite-host-resources"
        fun target(intent: Intent): String? = intent.getStringExtra(HOST)?.takeIf { it.isNotBlank() && it.length <= 160 }
    }
}
