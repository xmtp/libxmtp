package org.xmtp.android.example.messenger.session

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.SessionKey
import uniffi.xmtp_sdk.ConnectionState
import uniffi.xmtp_sdk.ErrorCategory
import uniffi.xmtp_sdk.ErrorDetails
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.MessageStreamOptions
import uniffi.xmtp_sdk.XmtpException
import java.util.Collections

// The default reader loop, with a fake SDK stream. Rust tests own the consumer
// lease; these check what the app does when another reader holds it.
class SessionReadersTest {
    private val key = SessionKey("profile", 1)
    private val connection = Collections.synchronizedList(mutableListOf<String>())
    private val failures = Collections.synchronizedList(mutableListOf<Throwable>())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val readers =
        SessionReaders(
            scope = scope,
            projection =
                object : SessionProjection {
                    override suspend fun handle(
                        owner: ActiveSession,
                        message: Message,
                    ) {}

                    override suspend fun invalidate(owner: ActiveSession) {}
                },
            accepts = { it == key },
            connection = { _, value -> connection.add(value) },
            readerFailure = { _, failure -> failures.add(failure) },
            conversationFailure = { _, failure -> failures.add(failure) },
        )

    private fun details(code: String) = ErrorDetails(code, ErrorCategory.STREAM, true, "failed")

    // A killed process can leave its lease until it expires. The reader shows
    // RECONNECTING, reports no reader error, and opens the stream again.
    @Test
    fun aHeldLeaseRetriesWithoutAReaderError() =
        runBlocking {
            val opens = mutableListOf<MessageStreamOptions>()
            val reopened = CompletableDeferred<Unit>()
            val open: (MessageStreamOptions) -> Flow<Message> = { options ->
                opens.add(options)
                flow {
                    if (opens.size == 1) throw XmtpException.ConsumerOwned(details("ConsumerOwned"))
                    options.onConnectionStateChange?.invoke(ConnectionState.RECONNECTING, ConnectionState.CONNECTED)
                    reopened.complete(Unit)
                    awaitCancellation()
                }
            }
            try {
                readers.startMessages(key, open) {}
                withTimeout(10_000) { while (!reopened.isCompleted && failures.isEmpty()) delay(10) }
                assertEquals(emptyList<Throwable>(), failures.toList())
                assertEquals(listOf("RECONNECTING", "CONNECTED"), connection.toList())
                assertEquals(2, opens.size)
                assertTrue(readers.messagesStarted)
            } finally {
                readers.stopMessages()
                scope.cancel()
            }
        }

    // Other stream failures stop the reader and report the error once.
    @Test
    fun anotherFailureStopsTheReader() =
        runBlocking {
            var opens = 0
            val failure = XmtpException.Unknown(details("Unknown"))
            try {
                val open: (MessageStreamOptions) -> Flow<Message> = {
                    flow {
                        opens++
                        throw failure
                    }
                }
                readers.startMessages(key, open) {}
                withTimeout(10_000) { while (failures.isEmpty()) delay(10) }
                assertEquals(listOf<Throwable>(failure), failures.toList())
                assertEquals(1, opens)
                assertEquals(emptyList<String>(), connection.toList())
            } finally {
                readers.stopMessages()
                scope.cancel()
            }
        }
}
