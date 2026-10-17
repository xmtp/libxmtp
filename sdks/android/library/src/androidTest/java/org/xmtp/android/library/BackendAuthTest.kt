package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicInteger

class BackendAuthTest : BaseInstrumentedTest() {
    private fun api(block: suspend () -> Credential) =
        localApi().copy(
            credentials =
                object : CredentialSource {
                    override suspend fun credential() = block()
                },
        )

    private fun source(options: BackendOptions) = BackendSource.Options(options)

    @Test fun rejectedCredentialIsRefreshedByNativeMiddleware() =
        runBlocking {
            assumeTrue(SDKClient.fetchServerConfiguration(source(localApi())).auth.enabled)
            val calls = AtomicInteger()
            val options =
                api {
                    val value = if (calls.incrementAndGet() == 1) "Bearer rejected-token" else AUTH_TEST_CREDENTIAL
                    Credential(null, value, Long.MAX_VALUE)
                }
            assertTrue(SDKClient.inboxIdFor(createWallet().identity(), source(options)).isNotBlank())
            assertEquals(2, calls.get())
        }

    @Test fun callbackFailureCrossesNativeBoundaryWithoutAppErrorText() =
        runBlocking {
            val calls = AtomicInteger()
            val options =
                api {
                    calls.incrementAndGet()
                    error("private-auth-error-token")
                }
            val failure =
                runCatching {
                    SDKClient.inboxIdFor(
                        createWallet().identity(),
                        source(options),
                    )
                }.exceptionOrNull()
            assertTrue(failure is XmtpException.CredentialCallbackFailed)
            assertTrue(calls.get() > 0)
            assertFalse(checkNotNull(failure).toString().contains("private-auth-error-token"))
        }

    @Test fun forwardsCallbacksAndKeepsConnectionsSeparate() =
        runBlocking {
            val firstCalls = AtomicInteger()
            val secondCalls = AtomicInteger()
            val firstOptions =
                api {
                    firstCalls.incrementAndGet()
                    Credential(null, AUTH_TEST_CREDENTIAL, Long.MAX_VALUE)
                }
            val secondOptions =
                api {
                    secondCalls.incrementAndGet()
                    Credential(null, AUTH_TEST_CREDENTIAL, Long.MAX_VALUE)
                }
            val first =
                Backend.connect(
                    firstOptions.copy(credentials = SDKForeign.credentials(checkNotNull(firstOptions.credentials))),
                )
            val repeated =
                Backend.connect(
                    firstOptions.copy(credentials = SDKForeign.credentials(checkNotNull(firstOptions.credentials))),
                )
            val second =
                Backend.connect(
                    secondOptions.copy(credentials = SDKForeign.credentials(checkNotNull(secondOptions.credentials))),
                )
            val anonymous = Backend.connect(localApi())
            try {
                assertNotSame(first, repeated)
                assertNotSame(first, second)
                assertNotSame(first, anonymous)
                SDKClient.inboxIdFor(createWallet().identity(), BackendSource.Connected(first))
                assertTrue(firstCalls.get() > 0)
                assertEquals(0, secondCalls.get())
                val previous = firstCalls.get()
                SDKClient.inboxIdFor(createWallet().identity(), BackendSource.Connected(second))
                assertEquals(previous, firstCalls.get())
                assertTrue(secondCalls.get() > 0)
                val created =
                    createClient(
                        createWallet(),
                        secondOptions.copy(credential = Credential(null, "private-static-token", Long.MAX_VALUE)),
                    )
                assertTrue(created.inboxId().isNotBlank())
                val projected = (created.options().backend as BackendSource.Options).options
                assertNull(projected.credential)
                assertNull(projected.credentials)
            } finally {
                first.close()
                repeated.close()
                second.close()
                anonymous.close()
            }
        }

    @Test fun discoveryDoesNotCallAuthentication() =
        runBlocking {
            var calls = 0
            val options =
                api {
                    calls++
                    error("Discovery must not call authentication")
                }
            assertTrue(SDKClient.fetchServerConfiguration(source(options)).identifier.isNotBlank())
            assertEquals(0, calls)
        }
}

private const val AUTH_TEST_CREDENTIAL = "Bearer sdk-auth-test-key-00000000000000000000"
