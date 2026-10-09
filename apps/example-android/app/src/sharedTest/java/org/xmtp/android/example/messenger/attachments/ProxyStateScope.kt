package org.xmtp.android.example.messenger.attachments

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/** Restores the caller's proxy state after this test releases its named hold. */
internal class ProxyStateScope(
    private val readEnabled: suspend () -> Boolean,
    private val writeEnabled: suspend (Boolean) -> Unit,
    private val releaseHold: suspend () -> Unit,
) {
    suspend fun <T> run(
        cleanup: suspend () -> Unit,
        block: suspend () -> T,
    ): T {
        var original: Boolean? = null
        try {
            original = readEnabled()
            return block()
        } finally {
            withContext(NonCancellable) {
                if (original == null) {
                    cleanup()
                } else {
                    try {
                        try {
                            releaseHold()
                        } finally {
                            cleanup()
                        }
                    } finally {
                        writeEnabled(original)
                    }
                }
            }
        }
    }
}
