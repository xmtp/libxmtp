package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder
import uniffi.xmtpv3.FfiException
import java.util.concurrent.atomic.AtomicInteger

@RunWith(AndroidJUnit4::class)
class BackendAuthTest : BaseInstrumentedTest() {
    @Test
    fun rejectedCredentialIsRefreshedByNativeMiddleware() =
        runBlocking {
            assumeTrue(Client.fetchServerConfiguration(localApi()).auth.enabled)
            val calls = AtomicInteger()
            val api =
                localApi().copy(authCallback = {
                    val value = if (calls.incrementAndGet() == 1) "Bearer rejected-token" else AUTH_TEST_CREDENTIAL
                    Credential(value, Long.MAX_VALUE)
                })
            assertTrue(Client.getOrCreateInboxId(api, PrivateKeyBuilder().publicIdentity).isNotBlank())
            assertEquals(2, calls.get())
        }

    @Test
    fun callbackFailureCrossesNativeBoundaryWithoutAppErrorText() =
        runBlocking {
            val calls = AtomicInteger()
            val api =
                localApi().copy(authCallback = {
                    calls.incrementAndGet()
                    error("private-auth-error-token")
                })
            try {
                Client.getOrCreateInboxId(api, PrivateKeyBuilder().publicIdentity)
                fail("Expected authentication failure")
            } catch (error: FfiException) {
                assertTrue(calls.get() > 0)
                assertFalse(error.toString().contains("private-auth-error-token"))
            }
        }

    @Test
    fun forwardsCallbacksAndKeepsConnectionsSeparate() =
        runBlocking {
            val firstCalls = AtomicInteger()
            val secondCalls = AtomicInteger()
            val firstApi =
                localApi().copy(authCallback = {
                    firstCalls.incrementAndGet()
                    Credential(AUTH_TEST_CREDENTIAL, Long.MAX_VALUE)
                })
            val secondApi =
                firstApi.copy(authCallback = {
                    secondCalls.incrementAndGet()
                    Credential(AUTH_TEST_CREDENTIAL, Long.MAX_VALUE)
                })
            val first = Client.connectToApiBackend(firstApi)
            val repeated = Client.connectToApiBackend(firstApi)
            val second = Client.connectToApiBackend(secondApi)
            val anonymous = Client.connectToApiBackend(localApi())
            try {
                assertNotSame(first, repeated)
                assertNotSame(first, second)
                assertNotSame(first, anonymous)
                Client.getOrCreateInboxId(firstApi, PrivateKeyBuilder().publicIdentity)
                assertTrue(firstCalls.get() > 0)
                assertEquals(0, secondCalls.get())
                val previousFirstCalls = firstCalls.get()
                Client.getOrCreateInboxId(secondApi, PrivateKeyBuilder().publicIdentity)
                assertEquals(previousFirstCalls, firstCalls.get())
                assertTrue(secondCalls.get() > 0)
                val previousSecondCalls = secondCalls.get()
                val client = createClient(PrivateKeyBuilder(), api = secondApi)
                assertTrue(client.inboxId.isNotBlank())
                assertTrue(secondCalls.get() > previousSecondCalls)
            } finally {
                first.close()
                repeated.close()
                second.close()
            }
        }

    @Test
    fun discoveryDoesNotCallAuthentication() =
        runBlocking {
            var calls = 0
            val api =
                localApi().copy(authCallback = {
                    calls++
                    error("Discovery must not call authentication")
                })
            assertTrue(Client.fetchServerConfiguration(api).identifier.isNotBlank())
            assertEquals(0, calls)
        }
}

// An auth-enabled test backend must accept this test-only static key.
private const val AUTH_TEST_CREDENTIAL = "Bearer sdk-auth-test-key-00000000000000000000"
