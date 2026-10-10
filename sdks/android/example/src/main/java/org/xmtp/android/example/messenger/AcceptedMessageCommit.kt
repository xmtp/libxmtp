package org.xmtp.android.example.messenger

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** A failed app commit must not turn SDK acceptance into a second typed send. */
internal class AcceptedMessageCommit(
    val messageId: String,
    private val persist: suspend () -> Boolean,
    private val verify: suspend () -> Boolean,
    private val acknowledge: () -> Unit,
) {
    private val lock = Mutex()
    private var acknowledged = false

    suspend fun finish(): Boolean =
        lock.withLock {
            if (acknowledged) return@withLock true
            if (!persist()) return@withLock false
            check(verify()) { "Stored message acceptance could not be verified. Refresh to recover it." }
            acknowledge()
            acknowledged = true
            true
        }
}
