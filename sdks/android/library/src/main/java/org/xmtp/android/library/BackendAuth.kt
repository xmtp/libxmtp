package org.xmtp.android.library

import kotlinx.coroutines.CancellationException
import uniffi.xmtpv3.FfiAuthCallback
import uniffi.xmtpv3.FfiAuthCallbackException
import uniffi.xmtpv3.FfiCredential

/** Supplies a credential when the backend connection needs one. Native code handles refresh and retry. */
typealias AuthCallback = suspend () -> Credential

/**
 * A backend credential. [expiresAtSeconds] is a Unix timestamp in seconds.
 * [name] defaults to the Authorization header. [value] is the full header value,
 * including `Bearer ` when the backend uses bearer tokens.
 */
class Credential(
    val value: String,
    val expiresAtSeconds: Long,
    val name: String? = null,
) {
    override fun toString(): String = "Credential(<redacted>)"

    internal fun toFfi(): FfiCredential = FfiCredential(name, value, expiresAtSeconds)
}

internal class BackendAuthCallback(
    private val callback: AuthCallback,
) : FfiAuthCallback {
    override suspend fun onAuthRequired(): FfiCredential =
        try {
            callback().toFfi()
        } catch (error: CancellationException) {
            throw error
        } catch (_: Throwable) {
            throw FfiAuthCallbackException.Failed()
        }
}
