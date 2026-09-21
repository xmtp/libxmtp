package org.xmtp.android.library

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Test
import uniffi.xmtpv3.FfiAuthCallbackException

class BackendAuthTest {
    @Test
    fun forwardsEachCredentialWithoutCaching() =
        runBlocking {
            var calls = 0
            val callback =
                BackendAuthCallback {
                    calls++
                    Credential("token-$calls", 123L + calls, "authorization")
                }
            val first = callback.onAuthRequired()
            val second = callback.onAuthRequired()
            assertEquals("authorization", first.name)
            assertEquals("token-1", first.value)
            assertEquals(124L, first.expiresAtSeconds)
            assertEquals("token-2", second.value)
            assertEquals(125L, second.expiresAtSeconds)
        }

    @Test
    fun defaultsHeaderAndRedactsCredential() {
        val credential = Credential("secret-token", 123L)
        assertNull(credential.toFfi().name)
        assertFalse(credential.toString().contains("secret-token"))
    }

    @Test
    fun hidesCallbackFailureAndPreservesCancellation() =
        runBlocking {
            try {
                BackendAuthCallback { error("secret-token") }.onAuthRequired()
                fail("Expected callback failure")
            } catch (error: FfiAuthCallbackException.Failed) {
                assertFalse(error.toString().contains("secret-token"))
                assertNull(error.cause)
            }
            val cancellation = CancellationException("cancelled")
            try {
                BackendAuthCallback { throw cancellation }.onAuthRequired()
                fail("Expected cancellation")
            } catch (error: CancellationException) {
                assertSame(cancellation, error)
            }
        }
}
