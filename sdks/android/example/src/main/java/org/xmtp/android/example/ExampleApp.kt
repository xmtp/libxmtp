package org.xmtp.android.example

import android.app.Application
import org.xmtp.android.example.messenger.AppSession

class ExampleApp : Application() {
    lateinit var session: AppSession
        private set

    override fun onCreate() {
        super.onCreate()
        session = AppSession(this)
    }
}
