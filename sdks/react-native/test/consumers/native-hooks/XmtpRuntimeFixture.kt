package com.xmtp.consumer.fixture

import android.os.Handler
import android.os.Looper
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import com.facebook.react.ReactApplication
import com.facebook.react.ReactPackage
import com.facebook.react.bridge.NativeModule
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.uimanager.ViewManager

/** Test application module. It is never included in the SDK npm product. */
class XmtpRuntimeFixture(
    context: ReactApplicationContext,
) : ReactContextBaseJavaModule(context) {
    companion object {
        private var launchURL = ""
        private var generation = 0

        fun prepareBackground(url: String) {
            check(generation == 0 && launchURL.isEmpty())
            launchURL = url
        }
    }

    override fun getName() = "XmtpRuntimeFixture"

    override fun getConstants(): Map<String, Any> = mapOf("launchURL" to launchURL, "generation" to generation)

    @ReactMethod
    fun processState(promise: Promise) {
        Handler(Looper.getMainLooper()).post {
            promise.resolve(
                if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(
                        Lifecycle.State.STARTED,
                    )
                ) {
                    "foreground"
                } else {
                    "background"
                },
            )
        }
    }

    @ReactMethod
    fun reload(
        url: String,
        expectedGeneration: Double,
        promise: Promise,
    ) {
        Handler(Looper.getMainLooper()).post {
            if (generation != expectedGeneration.toInt()) {
                promise.reject("STALE_GENERATION", "Fixture reload belongs to an old runtime")
                return@post
            }
            val application = reactApplicationContext.applicationContext as ReactApplication
            launchURL = url
            generation += 1
            promise.resolve(null)
            // Resolve the caller before destroying its runtime. The reload itself
            // uses ReactHost's release path, not a JS module reset or process restart.
            Handler(Looper.getMainLooper()).postDelayed({
                application.reactHost!!.reload("XMTP installed conformance")
            }, 100)
        }
    }
}

class XmtpRuntimeFixturePackage : ReactPackage {
    override fun createNativeModules(context: ReactApplicationContext): List<NativeModule> =
        listOf(XmtpRuntimeFixture(context))

    override fun createViewManagers(context: ReactApplicationContext): List<ViewManager<*, *>> = emptyList()
}
