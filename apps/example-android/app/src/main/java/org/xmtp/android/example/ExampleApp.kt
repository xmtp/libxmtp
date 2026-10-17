package org.xmtp.android.example

import android.app.Application
import androidx.emoji2.bundled.BundledEmojiCompatConfig
import androidx.emoji2.text.EmojiCompat
import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.app.AppGraph
import org.xmtp.android.example.messenger.notifications.NotificationController

class ExampleApp : Application() {
    internal lateinit var graph: AppGraph
    val session: AppSession get() = graph.session
    val notifications: NotificationController get() = graph.notifications

    override fun onCreate() {
        super.onCreate()
        EmojiCompat.init(BundledEmojiCompatConfig(this))
        graph = AppGraph(this)
    }
}
