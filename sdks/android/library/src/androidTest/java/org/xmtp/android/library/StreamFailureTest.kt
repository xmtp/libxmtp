package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtpv3.FfiException

@RunWith(AndroidJUnit4::class)
class StreamFailureTest {
    @Test
    fun readsTypedDetailsFromThePublicErrorProperty() {
        val message =
            "[BarrierError::Incomplete] failed\n[XMTP_STREAM_FAILURE_V1]" +
                """
                {"kind":"barrier","code":"BarrierError::Incomplete",
                 "message":"Processing barriers did not complete","retryable":true,
                 "intentId":null,"publishedIntentIds":[],"summary":null,
                 "barriers":[{"reason":"deadline","unfinished":[
                   {"topic":"01","target":"7","received":"7","processed":"6",
                    "unresolvedWelcomes":[],"inactive":false,"cause":null}]}]}
                """.trimIndent()
        val details = requireNotNull(FfiException.Exception(message).streamFailureDetails)
        assertEquals(StreamFailureKind.BARRIER, details.kind)
        assertEquals("BarrierError::Incomplete", details.code)
        assertEquals(StreamBarrierReason.DEADLINE, details.barriers.single().reason)
        assertEquals(
            7uL,
            details.barriers
                .single()
                .unfinished
                .single()
                .target,
        )
        assertEquals(
            6uL,
            details.barriers
                .single()
                .unfinished
                .single()
                .processed,
        )
        val wrapped = XMTPException("Unable to update group name", FfiException.Exception(message))
        val wrappedDetails = requireNotNull(wrapped.streamFailureDetails)
        assertEquals(details.kind, wrappedDetails.kind)
        assertEquals(details.code, wrappedDetails.code)
        assertEquals(
            7uL,
            wrappedDetails.barriers
                .single()
                .unfinished
                .single()
                .target,
        )
        assertNull(FfiException.Exception("ordinary error").streamFailureDetails)
        assertNull(IllegalStateException("ordinary error").streamFailureDetails)
    }
}
