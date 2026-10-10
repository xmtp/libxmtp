package org.xmtp.android.example.messenger

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.CredentialException

class SessionCredentialsTest {
    @Test fun missingAndEmptyInitialCredentialsDoNotCreateASource() =
        runBlocking {
            val fence = SessionFence()
            val key = checkNotNull(fence.replace("profile"))
            var bytes: ByteArray? = null
            val credentials = SessionCredentials(fence) { bytes }
            assertNull(credentials.forProfile("profile", key))
            bytes = byteArrayOf()
            assertNull(credentials.forProfile("profile", key))
        }

    @Test fun actualCallbackRereadsTheCapturedProfileAndKeepsCredentialFields() =
        runBlocking {
            val fence = SessionFence()
            val key = checkNotNull(fence.replace("profile"))
            val values = mutableMapOf("profile" to "initial".toByteArray(), "other" to "unrelated".toByteArray())
            val profiles = mutableListOf<String>()
            val credentials =
                SessionCredentials(fence) { profile ->
                    profiles.add(profile)
                    values[profile]
                }
            val source = checkNotNull(credentials.forProfile("profile", key))
            values["profile"] = "updated ütf8".toByteArray(Charsets.UTF_8)
            val value = source.credential()
            assertNull(value.name)
            assertEquals("updated ütf8", value.value)
            assertEquals(Long.MAX_VALUE, value.expiresAtSeconds)
            assertEquals(listOf("profile", "profile"), profiles)
            values["profile"] = byteArrayOf()
            assertEquals("", source.credential().value)
        }

    @Test fun actualCallbackRejectsAStaleOwnerBeforeReadingTheSecret() =
        runBlocking {
            val fence = SessionFence()
            val key = checkNotNull(fence.replace("profile"))
            var reads = 0
            val credentials =
                SessionCredentials(fence) {
                    reads += 1
                    "retained".toByteArray()
                }
            val source = checkNotNull(credentials.forProfile("profile", key))
            fence.reserve()
            assertTrue(runCatching { source.credential() }.exceptionOrNull() is CredentialException.Failed)
            assertEquals("A stale SDK callback must not read its captured secret", 1, reads)
        }

    @Test fun actualCallbackFailsWhenTheCapturedSecretWasRemoved() =
        runBlocking {
            val fence = SessionFence()
            val key = checkNotNull(fence.replace("profile"))
            var bytes: ByteArray? = "initial".toByteArray()
            val source = checkNotNull(SessionCredentials(fence) { bytes }.forProfile("profile", key))
            bytes = null
            assertTrue(runCatching { source.credential() }.exceptionOrNull() is CredentialException.Failed)
            assertTrue(fence.accepts(key))
        }
}
