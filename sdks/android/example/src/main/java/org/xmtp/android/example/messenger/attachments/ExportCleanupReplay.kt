package org.xmtp.android.example.messenger.attachments

/** A failed cleanup keeps its durable profile reference for the next startup. */
internal class ExportCleanupReplay(
    private val pending: suspend () -> Set<String>,
    private val cleanup: suspend (String) -> Unit,
    private val complete: suspend (String) -> Unit,
) {
    suspend fun run() {
        for (profile in pending()) {
            cleanup(profile)
            complete(profile)
        }
    }
}
