package org.xmtp.android.example.messenger

import uniffi.xmtp_sdk.Credential
import uniffi.xmtp_sdk.CredentialException
import uniffi.xmtp_sdk.CredentialSource

internal class SessionCredentials(
    private val fence: SessionFence,
    private val read: suspend (String) -> ByteArray?,
) {
    suspend fun forProfile(
        profile: String,
        key: SessionKey,
    ): CredentialSource? {
        if (read(profile)?.isNotEmpty() != true) return null
        return object : CredentialSource {
            override suspend fun credential(): Credential {
                if (!fence.accepts(key)) throw CredentialException.Failed()
                val bytes = read(profile) ?: throw CredentialException.Failed()
                return Credential(
                    name = null,
                    value = bytes.toString(Charsets.UTF_8),
                    expiresAtSeconds = Long.MAX_VALUE,
                )
            }
        }
    }
}
