package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamFailureFormattingTest {
    @Test
    fun defaultErrorTextKeepsTopicBytesInStructuredDetails() {
        val bytes = byteArrayOf(1) + ByteArray(32) { 0xab.toByte() }
        val topic =
            StreamBarrierTopic(bytes, 7uL, 42uL, 42uL, 41uL, emptyList(), false, null)
        val failure =
            StreamFailureDetails(
                StreamFailureKind.BARRIER,
                "BarrierError::Incomplete",
                "Processing barriers did not complete",
                true,
                null,
                emptyList(),
                null,
                listOf(StreamBarrierFailure(StreamBarrierReason.DEADLINE, listOf(topic))),
            )
        val details =
            ErrorDetails("Unknown", ErrorCategory.UNKNOWN, false, "Processing barrier did not complete", failure)
        val error = XmtpException.Unknown(details)
        assertFalse(error.message.orEmpty().contains(bytes.contentToString()))
        assertFalse(error.toString().contains(bytes.contentToString()))
        assertTrue(error.message.orEmpty().contains("<redacted>"))
        assertArrayEquals(
            bytes,
            details.streamFailure!!
                .barriers[0]
                .unfinished[0]
                .topic,
        )
    }
}
