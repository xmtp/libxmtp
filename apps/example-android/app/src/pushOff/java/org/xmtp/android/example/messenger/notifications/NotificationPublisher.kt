package org.xmtp.android.example.messenger.notifications

import android.app.Notification
import android.content.Context

internal object NotificationPublisher {
    fun post(
        context: Context,
        tag: String,
        notification: Notification,
    ): Boolean = false
}
