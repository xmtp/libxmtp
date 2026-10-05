package org.xmtp.android.library

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Test
import uniffi.xmtp_sdk.Credential
import uniffi.xmtp_sdk.CredentialException
import uniffi.xmtp_sdk.CredentialSource
import uniffi.xmtp_sdk.SDKForeign

class BackendAuthTest {
    @Test
    fun forwardsEachCredentialWithoutCaching() =
        runBlocking {
            var calls = 0
            val callback =
                credentialSource {
                    calls++
                    Credential("authorization", "token-$calls", 123L + calls)
                }
            val first = callback.credential()
            val second = callback.credential()
            assertEquals("authorization", first.name)
            assertEquals("token-1", first.value)
            assertEquals(124L, first.expiresAtSeconds)
            assertEquals("token-2", second.value)
            assertEquals(125L, second.expiresAtSeconds)
        }

    @Test
    fun defaultsHeaderAndRedactsCallbackFailure() =
        runBlocking {
            val credential = Credential(null, "secret-token", 123L)
            assertNull(credential.name)
            val error = runCatching { credentialSource { error(credential.value) }.credential() }.exceptionOrNull()
            assertFalse(checkNotNull(error).toString().contains(credential.value))
        }

    @Test
    fun hidesCallbackFailureAndPreservesCancellation() =
        runBlocking {
            try {
                credentialSource { error("secret-token") }.credential()
                fail("Expected callback failure")
            } catch (error: CredentialException.Failed) {
                assertFalse(error.toString().contains("secret-token"))
                assertNull(error.cause)
            }
            val cancellation = CancellationException("cancelled")
            try {
                credentialSource { throw cancellation }.credential()
                fail("Expected cancellation")
            } catch (error: CancellationException) {
                assertSame(cancellation, error)
            }
        }

    private fun credentialSource(block: suspend () -> Credential): CredentialSource =
        SDKForeign.credentials(
            object : CredentialSource {
                override suspend fun credential(): Credential = block()
            },
        )
}
