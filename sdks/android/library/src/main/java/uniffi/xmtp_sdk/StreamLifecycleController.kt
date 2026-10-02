package uniffi.xmtp_sdk

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
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
    private var transition: Job? = null

    fun setLive(live: Boolean) {
        val task =
            synchronized(lock) {
                desiredLive = live
                if (running || appliedLive == desiredLive) {
                    null
                } else {
                    running = true
                    scope.launch(start = CoroutineStart.LAZY) { reconcile() }.also { transition = it }
                }
            }
        task?.start()
    }

    suspend fun awaitSettled() {
        while (true) {
            val task = synchronized(lock) { if (running) transition else null } ?: return
            task.join()
        }
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
                synchronized(lock) {
                    if (target) {
                        running = false
                    } else {
                        // Native suspend sets the process latch before its fallible wait.
                        appliedLive = false
                    }
                }
                report(error)
                if (target) return
                continue
            }
            synchronized(lock) { appliedLive = target }
        }
    }
}
