package uniffi.xmtp_sdk

import android.util.Log
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.withContext

/**
 * Keep the shared transport in step with the process lifecycle.
 * Set [enabled] before the first Android client factory call to opt out.
 * The observer owns no client and lives for the process.
 */
object AndroidStreamLifecycle {
    @Volatile
    var enabled: Boolean = true

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val controller =
        StreamLifecycleController(scope, apply = { live ->
            if (live) resumeStreams() else suspendStreams()
        }, report = { error -> Log.w("XMTP", "Stream lifecycle transition failed", error) })
    private val startup =
        StreamLifecycleStartup(scope, controller) { setLive ->
            withContext(Dispatchers.Main.immediate) {
                val lifecycle = ProcessLifecycleOwner.get().lifecycle
                lifecycle.addObserver(
                    object : DefaultLifecycleObserver {
                        override fun onStart(owner: LifecycleOwner) = setLive(true)

                        override fun onStop(owner: LifecycleOwner) = setLive(false)
                    },
                )
                lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
            }
        }

    fun enable() {
        if (enabled) startup.enable()
    }

    internal suspend fun awaitReady() {
        if (enabled) startup.awaitReady()
    }
}
