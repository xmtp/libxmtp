package org.xmtp.android.example.messenger.notifications

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging

class NotificationTransport(private val context: Context) : PushTransport {
    override val configured = true
    override fun requestToken(callback: (String?, Throwable?) -> Unit) {
        try {
            FirebaseApp.initializeApp(context)
            FirebaseMessaging.getInstance().token.addOnCompleteListener { result ->
                if (result.isSuccessful) callback(result.result, null) else callback(null, result.exception)
            }
        } catch (error: Exception) { callback(null, error) }
    }
}
