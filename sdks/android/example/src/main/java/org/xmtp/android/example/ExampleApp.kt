package org.xmtp.android.example

import android.app.Application
import androidx.emoji2.bundled.BundledEmojiCompatConfig
import androidx.emoji2.text.EmojiCompat
import org.xmtp.android.example.messenger.AppSession

class ExampleApp : Application() {
    lateinit var session: AppSession
        private set

    override fun onCreate() {
        super.onCreate()
        EmojiCompat.init(BundledEmojiCompatConfig(this))
        session = AppSession(this)
    }
}
