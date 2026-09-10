package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.xmtpv3.FfiException

class StreamFailureTest {
    @Test
    fun passesTheRawNativeErrorMessageToTheTypedDecoder() {
        val message = "[BarrierError::Incomplete] failed\n[XMTP_STREAM_FAILURE_V1]{}"
        val details =
            StreamFailureDetails(
                kind = StreamFailureKind.BARRIER,
                code = "BarrierError::Incomplete",
                message = "Processing barriers did not complete",
                retryable = true,
                intentId = null,
                publishedIntentIds = emptyList(),
                summary = null,
                barriers = emptyList(),
            )
        var decodedMessage: String? = null
        val result =
            readStreamFailureDetails(FfiException.Exception(message)) {
                decodedMessage = it
                details
            }
        assertEquals(message, decodedMessage)
        assertSame(details, result)
    }

    @Test
    fun doesNotDecodeUnrelatedErrors() {
        var called = false
        val result =
            readStreamFailureDetails(IllegalStateException("ordinary error")) {
                called = true
                null
            }
        assertNull(result)
        assertEquals(false, called)
    }
}
