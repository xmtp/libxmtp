package org.xmtp.android.example.messenger.notifications

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat

internal object NotificationPublisher {
    fun post(
        context: Context,
        tag: String,
        notification: Notification,
    ): Boolean {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            return false
        }
        context.getSystemService(NotificationManager::class.java).notify(tag, 0, notification)
        return true
    }
}
