package uniffi.xmtp_sdk

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async

/** Register once and share completion of the initial native transition. */
internal class StreamLifecycleStartup(
    private val scope: CoroutineScope,
    private val controller: StreamLifecycleController,
    private val register: suspend ((Boolean) -> Unit) -> Boolean,
) {
    private val lock = Any()
    private var startup: Deferred<Unit>? = null

    fun enable() {
        task().start()
    }

    suspend fun awaitReady() {
        task().await()
    }

    private fun task(): Deferred<Unit> =
        synchronized(lock) {
            startup ?: scope
                .async(start = CoroutineStart.LAZY) {
                    val seedLock = Any()
                    var callbackSeen = false
                    val initial =
                        register { live ->
                            synchronized(seedLock) {
                                callbackSeen = true
                                controller.setLive(live)
                            }
                        }
                    synchronized(seedLock) {
                        if (!callbackSeen) controller.setLive(initial)
                    }
                    controller.awaitSettled()
                }.also { startup = it }
        }
}
