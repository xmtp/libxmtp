package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// SDKForeign.kt is hand-written Kotlin. Each wrapper turns a host failure into
// the error that its foreign trait declares and keeps cancellation.
// BackendAuthTest covers SDKForeign.credentials.
class ForeignWrapperTest {
    private val secret = "foreign-wrapper-secret"

    private fun assertHidden(error: Throwable?) {
        assertFalse(
            "The wrapper kept the host failure",
            generateSequence(error) { it.cause }.any { it is LinkageError || it.toString().contains(secret) },
        )
    }

    private suspend fun assertCancellation(action: suspend () -> Unit) {
        // Coroutine stack recovery can copy the exception, so compare its type and text.
        val error = runCatching { action() }.exceptionOrNull()
        assertTrue("Cancellation became $error", error is CancellationException)
        assertEquals("host cancelled", error?.message)
    }

    private class HostSigner(
        private val failure: Throwable,
    ) : Signer {
        override suspend fun identity(): PublicIdentity = throw failure

        override suspend fun kind(): SignerKind = throw failure

        override suspend fun sign(request: SigningRequest): Signature = throw failure
    }

    @Test
    fun signerFailuresAreSignerFailed() =
        runBlocking {
            val signer = SDKForeign.signer(HostSigner(LinkageError(secret)))
            for (call in listOf<suspend () -> Unit>({ signer.identity() }, { signer.kind() })) {
                val error = runCatching { call() }.exceptionOrNull()
                assertTrue("Expected SignerException.Failed, got $error", error is SignerException.Failed)
                assertHidden(error)
            }
            val cancelling = SDKForeign.signer(HostSigner(CancellationException("host cancelled")))
            assertCancellation { cancelling.identity() }
            assertCancellation { cancelling.kind() }
        }

    @Test
    fun logSinkFailuresAreLogSinkFailed() =
        runBlocking {
            val record = LogRecord(LogLevel.ERROR, "test", "message", emptyMap(), Timestamp(0), 0uL)
            val failing =
                SDKForeign.logSink(
                    object : LogSink {
                        override suspend fun log(record: LogRecord): Unit = throw LinkageError(secret)
                    },
                )
            val error = runCatching { failing.log(record) }.exceptionOrNull()
            assertTrue("Expected LogSinkException.Failed, got $error", error is LogSinkException.Failed)
            assertHidden(error)
            val cancelling =
                SDKForeign.logSink(
                    object : LogSink {
                        override suspend fun log(record: LogRecord): Unit =
                            throw CancellationException("host cancelled")
                    },
                )
            assertCancellation { cancelling.log(record) }
        }

    // The generated Credential display hides the token and keeps the value.
    @Test
    fun credentialTextHidesItsToken() {
        val credential = Credential(null, "Bearer $secret", 123L)
        assertEquals("Bearer $secret", credential.value)
        assertFalse("Credential display exposed its token", credential.toString().contains(secret))
    }
}
