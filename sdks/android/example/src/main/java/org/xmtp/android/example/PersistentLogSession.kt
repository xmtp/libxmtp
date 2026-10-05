package org.xmtp.android.example

import uniffi.xmtp_sdk.LogLevel

/** Keep DEBUG admission within the file logging session. */
internal class PersistentLogSession(
    private val setLevel: suspend (LogLevel) -> Unit,
    private val enter: () -> Unit,
    private val exit: () -> Unit,
) {
    suspend fun activate() {
        try {
            // The first call fixes the native layer at INFO.
            setLevel(LogLevel.INFO)
            setLevel(LogLevel.DEBUG)
            enter()
        } catch (error: Throwable) {
            resetLevel(error)
            throw error
        }
    }

    suspend fun deactivate() {
        val error =
            try {
                exit()
                null
            } catch (error: Throwable) {
                error
            }
        resetLevel(error)
        if (error != null) throw error
    }

    private suspend fun resetLevel(original: Throwable?) {
        try {
            setLevel(LogLevel.INFO)
        } catch (resetError: Throwable) {
            if (original == null) throw resetError
            if (resetError !== original) original.addSuppressed(resetError)
        }
    }
}
