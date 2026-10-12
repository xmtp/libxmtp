package org.xmtp.android.example.messenger.session

import android.util.Log

/** Only a fixed code and exception type may leave the session error boundary. */
fun interface SessionDiagnosticSink {
    fun report(
        code: String,
        failureType: String,
    )
}

internal object LocalSessionDiagnostics : SessionDiagnosticSink {
    override fun report(
        code: String,
        failureType: String,
    ) {
        Log.w("MessengerSession", "$code ($failureType)")
    }
}
