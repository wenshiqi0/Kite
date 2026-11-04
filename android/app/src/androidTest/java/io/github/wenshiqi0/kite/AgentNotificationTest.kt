package io.github.wenshiqi0.kite

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import androidx.core.app.NotificationCompat
import androidx.test.platform.app.InstrumentationRegistry
import io.github.wenshiqi0.kite.core.AgentNoticeInfo
import io.github.wenshiqi0.kite.core.AgentNoticeKind
import io.github.wenshiqi0.kite.data.AgentNotifications
import org.junit.Assert.*
import org.junit.Test

/** Actual Android NotificationManager checks. Does not send a turn or require Firebase. */
class AgentNotificationTest {
    @Test fun deliveryPrivacyDeduplicationAndDisablePreserveConnectionNotice() {
        val test=InstrumentationRegistry.getInstrumentation();val context=test.targetContext
        val notifier=AgentNotifications(context)
        val manager=context.getSystemService(NotificationManager::class.java)
        val enabled=notifier.enabled
        val id="owned-notification-test/"+"a".repeat(64)
        val channel="kite-owned-connection-test"
        manager.createNotificationChannel(NotificationChannel(channel,"Owned test",NotificationManager.IMPORTANCE_LOW))
        val notice=AgentNoticeInfo(id,"owned-notification-test","session","Owned notification validation",AgentNoticeKind.QUESTION,System.currentTimeMillis(),false,false)
        try {
            assertTrue("Allow POST_NOTIFICATIONS before this owned check",notifier.permissionGranted())
            notifier.enable(true)
            manager.notify("owned-connection",991,NotificationCompat.Builder(context,channel).setSmallIcon(R.drawable.ic_launcher_foreground).setContentTitle("Owned connection check").build())
            assertTrue(notifier.deliver(notice));assertFalse(notifier.deliver(notice))
            val deliveryDeadline=android.os.SystemClock.uptimeMillis()+2_000
            while(manager.activeNotifications.none{it.tag==id}&&android.os.SystemClock.uptimeMillis()<deliveryDeadline)android.os.SystemClock.sleep(20)
            assertTrue("System notification delivery completes",manager.activeNotifications.any{it.tag==id})
            val posted=manager.activeNotifications.single{it.tag==id}
            assertEquals(AgentNotifications.ATTENTION,posted.notification.channelId)
            assertEquals(Notification.VISIBILITY_PRIVATE,posted.notification.visibility)
            assertEquals(context.getString(R.string.notice_private),posted.notification.publicVersion.extras.getString(Notification.EXTRA_TEXT))
            assertEquals(context.packageName,posted.notification.contentIntent.creatorPackage)
            assertTrue(posted.notification.flags and Notification.FLAG_AUTO_CANCEL != 0)
            test.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS)
            android.os.SystemClock.sleep(400)
            java.io.File(context.getExternalFilesDir(null),"agent-notification-drawer.png").outputStream().use{test.uiAutomation.takeScreenshot().compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
            notifier.enable(false)
            assertFalse(notifier.deliver(notice))
            val cancelDeadline=android.os.SystemClock.uptimeMillis()+2_000
            while(manager.activeNotifications.any{it.tag==id}&&android.os.SystemClock.uptimeMillis()<cancelDeadline)android.os.SystemClock.sleep(20)
            assertFalse(manager.activeNotifications.any{it.tag==id})
            assertTrue(manager.activeNotifications.any{it.tag=="owned-connection"})
        }finally{
            manager.cancel(id,2);manager.cancel("owned-connection",991);manager.deleteNotificationChannel(channel)
            notifier.enable(enabled)
            test.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
        }
    }
}
