package org.xmtp.android.example.messenger.attachments

import android.app.Activity
import android.app.Service
import android.content.Intent
import android.os.*

/** Runs as the test APK's UID, which differs from the app UID. */
class FileGrantReceiverActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val receiver = intent.getParcelableExtra<ResultReceiver>("result")
        val readable =
            try {
                contentResolver.openInputStream(checkNotNull(intent.data))?.use { it.read() >= 0 } == true
            } catch (
                _: Exception,
            ) {
                false
            }
        receiver?.send(if (readable) 1 else 0, Bundle())
        finish()
    }
}

class FileGrantReceiverService : Service() {
    private val receiver by lazy {
        Messenger(
            object : Handler(Looper.getMainLooper()) {
                override fun handleMessage(message: Message) {
                    val readable =
                        try {
                            contentResolver.openInputStream(android.net.Uri.parse(message.data.getString("uri")))?.use {
                                it.read() >=
                                    0
                            } ==
                                true
                        } catch (_: Exception) {
                            false
                        }
                    message.replyTo.send(Message.obtain().apply { arg1 = if (readable) 1 else 0 })
                }
            },
        )
    }

    override fun onBind(intent: Intent): IBinder = receiver.binder
}
