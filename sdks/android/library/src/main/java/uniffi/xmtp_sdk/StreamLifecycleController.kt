package uniffi.xmtp_sdk

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** Serialize transitions. A later process state replaces pending intent. */
internal class StreamLifecycleController(
    private val scope: CoroutineScope,
    private val apply: suspend (Boolean) -> Unit,
    private val report: (Throwable) -> Unit,
) {
    private val lock = Any()
    private var desiredLive = true
    private var appliedLive = true
    private var running = false

    fun setLive(live: Boolean) {
        val start =
            synchronized(lock) {
                desiredLive = live
                (!running && appliedLive != desiredLive).also { if (it) running = true }
            }
        if (start) scope.launch { reconcile() }
    }

    private suspend fun reconcile() {
        while (true) {
            val target =
                synchronized(lock) {
                    if (desiredLive == appliedLive) {
                        running = false
                        return
                    }
                    desiredLive
                }
            try {
                apply(target)
            } catch (error: Throwable) {
                synchronized(lock) { running = false }
                report(error)
                return
            }
            synchronized(lock) { appliedLive = target }
        }
    }
}
