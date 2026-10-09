package org.xmtp.android.example.messenger.notifications

interface PushTransport {
    val configured: Boolean

    fun requestToken(callback: (String?, Throwable?) -> Unit)
}
