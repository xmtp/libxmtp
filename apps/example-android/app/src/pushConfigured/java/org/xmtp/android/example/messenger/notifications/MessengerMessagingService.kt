package org.xmtp.android.example.messenger.notifications

import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.xmtp.android.example.ExampleApp

class MessengerMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        (application as ExampleApp).notifications.tokenChanged(token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        runBlocking { withTimeoutOrNull(9_000) { (application as ExampleApp).notifications.receive(message.data) } }
    }
}
