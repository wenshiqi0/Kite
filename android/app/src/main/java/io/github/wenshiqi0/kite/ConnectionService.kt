package io.github.wenshiqi0.kite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.IBinder
import android.os.Build
import android.content.pm.ServiceInfo
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/** Native foreground lifecycle only; SSH and protocol work remain in Rust. */
class ConnectionService : Service() {
    private val engine get() = (application as KiteApplication).engine
    private var video = false

    fun setVideoActive(active: Boolean) {
        if (video != active) { video = active; promote() }
    }

    private fun promote() {
        val type = if (Build.VERSION.SDK_INT >= 29) {
            if (video) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
        } else 0
        ServiceCompat.startForeground(this, 1, notification(), type)
    }

    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.connection_channel), NotificationManager.IMPORTANCE_LOW),
        )
        promote()
        engine.foregroundReady(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == DISCONNECT) engine.command { engine.core.disconnect() }
        return START_NOT_STICKY
    }

    private fun notification(): Notification {
        val openIntent = Intent(this, MainActivity::class.java).setAction(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER).addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val open = PendingIntent.getActivity(this, 0, openIntent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val disconnect = PendingIntent.getService(this, 1, Intent(this, ConnectionService::class.java).setAction(DISCONNECT), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(getString(R.string.connection_notification_title))
            .setContentText(getString(R.string.connection_notification_text))
            .setContentIntent(open).setOngoing(true).setSilent(true)
            .addAction(0, getString(R.string.disconnect), disconnect).build()
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        engine.foregroundTimedOut()
        stopSelf()
    }

    override fun onDestroy() {
        engine.foregroundStopped(this)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val DISCONNECT = "io.github.wenshiqi0.kite.DISCONNECT"
        private const val CHANNEL = "kite-connection"
    }
}
