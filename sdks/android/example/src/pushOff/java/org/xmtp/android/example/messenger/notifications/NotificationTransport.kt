package org.xmtp.android.example.messenger.notifications

import android.content.Context

class NotificationTransport(
    context: Context,
) : PushTransport {
    override val configured = false

    override fun requestToken(callback: (String?, Throwable?) -> Unit) = Unit
}
