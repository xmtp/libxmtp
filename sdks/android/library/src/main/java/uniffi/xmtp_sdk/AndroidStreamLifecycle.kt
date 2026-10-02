package uniffi.xmtp_sdk

import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * Keep the shared transport in step with the process lifecycle.
 * Set [enabled] before the first Android client factory call to opt out.
 * The observer owns no client and lives for the process.
 */
object AndroidStreamLifecycle {
    @Volatile
    var enabled: Boolean = true

    private val lock = Any()
    private var registered = false
    private val controller =
        StreamLifecycleController(CoroutineScope(SupervisorJob() + Dispatchers.IO), apply = { live ->
            if (live) resumeStreams() else suspendStreams()
        }, report = { error -> Log.w("XMTP", "Stream lifecycle transition failed", error) })

    fun enable() {
        synchronized(lock) {
            if (!enabled || registered) return
            registered = true
        }
        Handler(Looper.getMainLooper()).post {
            val lifecycle = ProcessLifecycleOwner.get().lifecycle
            lifecycle.addObserver(
                object : DefaultLifecycleObserver {
                    override fun onStart(owner: LifecycleOwner) = controller.setLive(true)

                    override fun onStop(owner: LifecycleOwner) = controller.setLive(false)
                },
            )
            controller.setLive(lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED))
        }
    }
}
