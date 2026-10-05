package org.xmtp.android.library

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.io.ByteArrayOutputStream
import java.io.PrintStream

class CloseCallbackLogTest {
    @Test(timeout = 10_000L)
    fun closeCallbackErrorKeepsItsTextOutOfDiagnostics() =
        runBlocking {
            val secret = "CLOSE_CALLBACK_SECRET_6ed07"
            for (failure in listOf<Throwable?>(null, streamFailure(), CancellationException("cancelled"))) {
                val reader =
                    RecordingMessageReader {
                        failure?.let { throw it }
                        null
                    }
                val client = testSDKClient(RecordingReaderClient { reader })
                val output = ByteArrayOutputStream()
                val previous = System.err
                var closeCalls = 0
                var reason: SDKStreamCloseReason? = null
                val received =
                    try {
                        System.setErr(PrintStream(output, true, "UTF-8"))
                        runCatching {
                            client
                                .messages(onClose = {
                                    closeCalls += 1
                                    reason = it
                                    throw IllegalStateException(secret)
                                })
                                .collect {}
                        }.exceptionOrNull()
                    } finally {
                        System.setErr(previous)
                    }
                assertSame(failure, received)
                assertEquals(1, reader.nextCalls)
                assertEquals(1, reader.endCalls)
                assertEquals(1, closeCalls)
                if (failure == null || failure is CancellationException) {
                    assertEquals(SDKStreamCloseReason.Closed, reason)
                } else {
                    assertSame(failure, (reason as SDKStreamCloseReason.Failed).error)
                }
                val diagnostic = output.toString("UTF-8")
                assertFalse("Close callback diagnostics expose the test secret", diagnostic.contains(secret))
                assertTrue(diagnostic.contains("XMTP stream close callback failed"))
            }
        }
}
