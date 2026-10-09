package org.xmtp.android.example.messenger.notifications

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assume.assumeTrue
import org.junit.Test

class NotificationPublisherInstrumentedTest {
    @Test fun configuredPublisherRejectsDeniedAndroidPermission() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(Build.VERSION.SDK_INT >= 33)
        val permissions = context.packageManager.getPackageInfo(context.packageName, PackageManager.GET_PERMISSIONS)
        assumeTrue(permissions.requestedPermissions?.contains(Manifest.permission.POST_NOTIFICATIONS) == true)
        assumeTrue(
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED,
        )
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel("messages", "Permission test", NotificationManager.IMPORTANCE_DEFAULT),
        )
        println("PUSH_PERMISSION api=${Build.VERSION.SDK_INT} declared=true denied=true channel_created=true")
        val notification =
            Notification
                .Builder(
                    context,
                    "messages",
                ).setSmallIcon(android.R.drawable.ic_dialog_email)
                .build()
        val posted =
            try {
                NotificationPublisher.post(context, "permission-test", notification)
            } catch (error: Exception) {
                println("PUSH_PERMISSION thrown=${error.javaClass.name}")
                throw error
            }
        println("PUSH_PERMISSION returned=$posted own_active_notifications=${manager.activeNotifications.size}")
        assertFalse(posted)
    }
}
