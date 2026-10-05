package org.xmtp.android.example

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** Order writer changes and saved state across Activity instances. */
internal class LogWriterController(
    private val scope: CoroutineScope,
    private val activate: suspend () -> Unit,
    private val deactivate: () -> Unit,
    private val isActivated: () -> Boolean,
    private val saveActivated: (Boolean) -> Unit,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val mutex = Mutex()

    fun setActivated(
        activated: Boolean,
        restoreOnly: Boolean = false,
    ): Deferred<Unit> =
        // Take a place in the queue before returning to the caller.
        scope.async(start = CoroutineStart.UNDISPATCHED) {
            mutex.withLock {
                withContext(dispatcher) {
                    if (restoreOnly && !isActivated()) return@withContext
                    if (activated) {
                        try {
                            activate()
                        } catch (error: Throwable) {
                            saveActivated(false)
                            throw error
                        }
                    } else {
                        deactivate()
                    }
                    saveActivated(activated)
                }
            }
        }
}
