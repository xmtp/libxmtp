package org.xmtp.android.library

import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.nio.ByteBuffer

class StreamFailureTest {
    // verifies: PROC-018
    // Native operation failures need a separate installed proof.
    @Test fun readsTypedDetailsFromThePublicErrorProperty() {
        val failure =
            StreamFailureDetails(
                StreamFailureKind.BARRIER,
                "BarrierError::Incomplete",
                "Processing barriers did not complete",
                true,
                null,
                emptyList(),
                null,
                listOf(
                    StreamBarrierFailure(
                        StreamBarrierReason.DEADLINE,
                        listOf(
                            StreamBarrierTopic(
                                byteArrayOf(1),
                                ULong.MAX_VALUE,
                                7uL,
                                7uL,
                                6uL,
                                listOf(9_007_199_254_740_993uL, ULong.MAX_VALUE),
                                false,
                                StreamBarrierCause(
                                    StreamBarrierCauseKind.PROCESSING_PENDING,
                                    null,
                                    "Processing is pending",
                                    true,
                                ),
                            ),
                        ),
                    ),
                ),
            )
        val error = ErrorDetails("Unknown", ErrorCategory.STREAM, true, "failed", failure)
        val buffer = ByteBuffer.allocate(FfiConverterTypeErrorDetails.allocationSize(error).toInt())
        FfiConverterTypeErrorDetails.write(error, buffer)
        buffer.flip()
        val decoded = FfiConverterTypeErrorDetails.read(buffer)
        val publicError = XmtpException.Unknown(decoded)
        val details = requireNotNull(publicError.v1.streamFailure)
        assertEquals(failure, details)
        assertEquals(StreamFailureKind.BARRIER, details.kind)
        assertEquals("BarrierError::Incomplete", details.code)
        assertEquals(StreamBarrierReason.DEADLINE, details.barriers.single().reason)
        val topic =
            details.barriers
                .single()
                .unfinished
                .single()
        assertArrayEquals(byteArrayOf(1), topic.topic)
        assertEquals(ULong.MAX_VALUE, topic.scopeGeneration)
        assertEquals(7uL, topic.target)
        assertEquals(7uL, topic.received)
        assertEquals(6uL, topic.processed)
        assertEquals(listOf(9_007_199_254_740_993uL, ULong.MAX_VALUE), topic.unresolvedWelcomes)
        assertEquals(StreamBarrierCauseKind.PROCESSING_PENDING, topic.cause?.kind)
        val wrapped = IllegalStateException("Unable to update group name", publicError)
        assertEquals(details, (wrapped.cause as XmtpException.Unknown).v1.streamFailure)
        assertNull(
            XmtpException
                .Unknown(
                    ErrorDetails(
                        "Unknown",
                        ErrorCategory.UNKNOWN,
                        false,
                        "ordinary error",
                    ),
                ).v1.streamFailure,
        )
    }
}
