package com.xmtp.consumer.fixture

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import com.facebook.react.ReactApplication
import com.facebook.react.ReactInstanceEventListener
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.ReactContext
import com.facebook.react.jstasks.HeadlessJsTaskConfig
import com.facebook.react.jstasks.HeadlessJsTaskContext

/** Starts Hermes from a cold background process without creating an Activity. */
class XmtpBackgroundReceiver : BroadcastReceiver() {
    override fun onReceive(
        context: Context,
        intent: Intent,
    ) {
        val url = intent.getStringExtra("url") ?: error("background fixture requires its control URL")
        XmtpRuntimeFixture.prepareBackground(url)
        val pending = goAsync()
        val host = (context.applicationContext as ReactApplication).reactHost!!

        fun start(reactContext: ReactContext) {
            val data = Arguments.createMap().apply { putString("url", url) }
            HeadlessJsTaskContext.getInstance(reactContext).startTask(
                HeadlessJsTaskConfig("XmtpBackgroundFixture", data, 180_000, false),
            )
            pending.finish()
        }
        val current = host.currentReactContext
        if (current != null) {
            start(current)
            return
        }
        val listener =
            object : ReactInstanceEventListener {
                override fun onReactContextInitialized(reactContext: ReactContext) {
                    host.removeReactInstanceEventListener(this)
                    Handler(Looper.getMainLooper()).post { start(reactContext) }
                }
            }
        host.addReactInstanceEventListener(listener)
        host.start()
    }
}
