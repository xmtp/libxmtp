package uniffi.xmtp_sdk

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.RecordingMessageReader
import org.xmtp.android.library.RecordingReaderClient
import org.xmtp.android.library.deliveryTestMessage
import org.xmtp.android.library.testSDKClient
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

private const val STREAM_TEST_TIMEOUT_MS = 10_000L

// The Flow adapter in streams/Readers.kt and the stream forms in SDKClient.kt
// are hand-written Kotlin. The native boundary here is a recording fake; Rust
// tests own native acknowledgement and connection state
// (xmtp_sdk/src/tests/reader_ack_cancellation.rs).
class MessageStreamTest {
    /** A reader whose connection state the test controls. */
    private class StateReader(
        private val initial: ConnectionState,
        private val changed: suspend (ConnectionState) -> ConnectionState,
    ) : MessageReader(NoHandle) {
        var endCalls = 0

        override suspend fun next(): Message? = awaitCancellation()

        override suspend fun end() {
            endCalls++
        }

        override suspend fun connectionState() = initial

        override suspend fun connectionStateChanged(previous: ConnectionState) = changed(previous)
    }

    /** A Group and a Dm that record the options of each reader they open. */
    private class RecordingGroup(
        private val open: suspend () -> MessageReader,
    ) : Group(NoHandle) {
        val options = mutableListOf<ConversationMessageReaderOptions?>()

        override suspend fun messageReader(options: ConversationMessageReaderOptions?): MessageReader {
            this.options += options
            return open()
        }
    }

    private class RecordingDm(
        private val open: suspend () -> MessageReader,
    ) : Dm(NoHandle) {
        val options = mutableListOf<ConversationMessageReaderOptions?>()

        override suspend fun messageReader(options: ConversationMessageReaderOptions?): MessageReader {
            this.options += options
            return open()
        }
    }

    /** A conversation reader that records each host call. */
    private class RecordingConversationReader(
        private val read: suspend () -> Conversation?,
    ) : ConversationReader(NoHandle) {
        var nextCalls = 0
        var endCalls = 0

        override suspend fun next(): Conversation? {
            nextCalls++
            return read()
        }

        override suspend fun end() {
            endCalls++
        }

        override suspend fun connectionState() = ConnectionState.CONNECTED

        override suspend fun connectionStateChanged(previous: ConnectionState): ConnectionState = awaitCancellation()
    }

    /** A native client whose conversations open [open] and record the options. */
    private class ConversationReaderClient(
        private val open: suspend () -> ConversationReader,
    ) : Client(NoHandle) {
        val options = mutableListOf<ConversationReaderOptions?>()
        private val conversations =
            object : Conversations(NoHandle) {
                override suspend fun conversationReader(options: ConversationReaderOptions?): ConversationReader {
                    this@ConversationReaderClient.options += options
                    return open()
                }
            }

        override fun clientKey(): ULong = 1uL

        override fun conversations(): Conversations = conversations
    }

    private fun ownerClient() =
        testSDKClient(RecordingReaderClient { error("only the conversation forms open readers") })

    // The 4c handoff: no other Android test calls messages(group, options) or messages(dm, options).
    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun conversationFormsOpenTheirReaderWithTheCallerOptions() =
        runBlocking {
            val client = ownerClient()
            val options = ConversationMessageReaderOptions(from = "cursor-from")
            val group = RecordingGroup { RecordingMessageReader { null } }
            client.messages(group, options).collect()
            client.messages(group).collect()
            assertEquals(listOf(options, null), group.options)
            val dm = RecordingDm { RecordingMessageReader { null } }
            client.messages(dm, options).collect()
            client.messages(dm).collect()
            assertEquals(listOf(options, null), dm.options)
        }

    // conversationStream has its own hand-written wiring in SDKClient.kt and
    // streams/Readers.kt (conversationFlow): the options, the reader end and onClose.
    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun conversationStreamEndsItsReaderAndReportsTheCloseOnce() =
        runBlocking {
            val rows = mutableListOf<Conversation>(Conversation.Group(Group(NoHandle)), Conversation.Dm(Dm(NoHandle)))
            val reader = RecordingConversationReader { rows.removeFirstOrNull() }
            val raw = ConversationReaderClient { reader }
            val client = testSDKClient(raw)
            val closes = mutableListOf<SDKStreamCloseReason>()
            val received =
                client
                    .conversationStream(
                        kind = ConversationKind.GROUP,
                        consentStates = listOf(ConsentState.ALLOWED),
                        onClose = { closes.add(it) },
                    ).take(1)
                    .toList()
            assertTrue(received.single() is Conversation.Group)
            assertEquals(
                listOf(ConversationReaderOptions(ConversationKind.GROUP, listOf(ConsentState.ALLOWED))),
                raw.options,
            )
            assertEquals(1, reader.nextCalls)
            assertEquals("The conversation reader was not ended", 1, reader.endCalls)
            assertEquals(listOf<SDKStreamCloseReason>(SDKStreamCloseReason.Closed), closes)
        }

    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun throwingConversationOnCloseDoesNotEscapeTheCollection() =
        runBlocking {
            for (failure in listOf<Throwable?>(null, IllegalStateException("conversation read failed"))) {
                val reader =
                    RecordingConversationReader {
                        failure?.let { throw it }
                        null
                    }
                val client = testSDKClient(ConversationReaderClient { reader })
                val closes = mutableListOf<SDKStreamCloseReason>()
                val received =
                    runCatching {
                        client
                            .conversationStream(onClose = {
                                closes.add(it)
                                throw IllegalStateException("close callback failed")
                            })
                            .collect()
                    }.exceptionOrNull()
                assertSame("The close callback error escaped the collection", failure, received)
                assertEquals(1, reader.endCalls)
                if (failure == null) {
                    assertEquals(listOf<SDKStreamCloseReason>(SDKStreamCloseReason.Closed), closes)
                } else {
                    assertSame(failure, (closes.single() as SDKStreamCloseReason.Failed).error)
                }
            }
        }

    // verifies: PROC-041
    // An early collector exit closes once with Closed.
    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun earlyExitClosesAfterOneReadWithoutAnotherRequest() =
        runBlocking {
            for (form in listOf("first", "take")) {
                val rows = mutableListOf(deliveryTestMessage(), deliveryTestMessage(id = "05".repeat(32)))
                val reader = RecordingMessageReader { rows.removeFirstOrNull() }
                val client = testSDKClient(RecordingReaderClient { reader })
                val closes = mutableListOf<SDKStreamCloseReason>()
                val flow = client.messages(onClose = { closes.add(it) })
                val id =
                    if (form == "first") {
                        flow.first().id
                    } else {
                        flow
                            .take(1)
                            .toList()
                            .single()
                            .id
                    }
                assertEquals(form, "01".repeat(32), id)
                // The next request acknowledges a value; an early exit makes none.
                assertEquals(form, 1, reader.nextCalls)
                assertEquals(form, 1, reader.endCalls)
                assertEquals(form, listOf(SDKStreamCloseReason.Closed), closes)
            }
        }

    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun endedOwnerReportsClientClosedWithoutAHandoff() =
        runBlocking {
            val rows = mutableListOf(deliveryTestMessage())
            val reader = RecordingMessageReader { rows.removeFirstOrNull() }
            val raw = RecordingReaderClient { reader }
            val client = testSDKClient(raw)
            raw.closed = true
            val closes = mutableListOf<SDKStreamCloseReason>()
            val delivered = mutableListOf<Message>()
            val failure =
                runCatching { client.messages(onClose = { closes.add(it) }).collect { delivered.add(it) } }
                    .exceptionOrNull()
            assertTrue("Expected ClientClosed, got $failure", failure is XmtpException.ClientClosed)
            assertTrue(delivered.isEmpty())
            assertEquals(0, reader.nextCalls)
            assertSame(failure, (closes.single() as SDKStreamCloseReason.Failed).error)
        }

    // A collection cancelled while its reader opens must end that late reader,
    // and must report the close only after it ends.
    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun lateReaderOpenedAfterCancellationIsEndedBeforeTheClose() =
        runBlocking {
            val opening = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val late = RecordingMessageReader { error("a cancelled collection must not read") }
            val group =
                RecordingGroup {
                    opening.complete(Unit)
                    release.await()
                    late
                }
            val client = ownerClient()
            val closed = CompletableDeferred<SDKStreamCloseReason>()
            val collection =
                async(start = CoroutineStart.UNDISPATCHED) {
                    client.messages(group, onClose = { closed.complete(it) }).collect {}
                }
            withTimeout(5_000) { opening.await() }
            collection.cancel()
            delay(100)
            assertFalse("The close ran before the late reader ended", closed.isCompleted)
            assertEquals(0, late.endCalls)
            release.complete(Unit)
            withTimeout(5_000) { collection.join() }
            assertEquals(SDKStreamCloseReason.Closed, withTimeout(5_000) { closed.await() })
            assertEquals(1, late.endCalls)
            assertEquals(0, late.nextCalls)
        }

    // verifies: PROC-044
    // A reader opened on a connected connection reports Connected first.
    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun connectedReaderReportsConnectedFirst() =
        runBlocking {
            val reader = StateReader(ConnectionState.CONNECTED) { awaitCancellation() }
            val client = testSDKClient(RecordingReaderClient { reader })
            val first = CompletableDeferred<Pair<ConnectionState?, ConnectionState>>()
            val collection =
                launch {
                    client
                        .messages(onConnectionStateChange = {
                            previous,
                            current,
                            ->
                            first.complete(previous to current)
                        })
                        .collect {}
                }
            try {
                assertEquals(null to ConnectionState.CONNECTED, withTimeout(5_000) { first.await() })
            } finally {
                collection.cancelAndJoin()
            }
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun stateMonitorStopsReadingAfterClosed() =
        runBlocking {
            val reads = AtomicInteger()
            val reader =
                StateReader(ConnectionState.CONNECTING) {
                    reads.incrementAndGet()
                    ConnectionState.CLOSED
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            val states = mutableListOf<Pair<ConnectionState?, ConnectionState>>()
            val closed = CompletableDeferred<Unit>()
            val collection =
                launch {
                    client
                        .messages(onConnectionStateChange = { previous, current ->
                            synchronized(states) { states.add(previous to current) }
                            if (current == ConnectionState.CLOSED) closed.complete(Unit)
                        })
                        .collect {}
                }
            try {
                withTimeout(5_000) { closed.await() }
                val atClosed = reads.get()
                delay(100)
                assertEquals("The state monitor kept reading after Closed", atClosed, reads.get())
                assertEquals(
                    listOf(null to ConnectionState.CONNECTING, ConnectionState.CONNECTING to ConnectionState.CLOSED),
                    synchronized(states) { states.toList() },
                )
            } finally {
                collection.cancelAndJoin()
            }
        }

    @Test(timeout = STREAM_TEST_TIMEOUT_MS)
    fun throwingStateCallbackKeepsTheStreamAndTheProcess() =
        runBlocking {
            val stateCalled = CompletableDeferred<Unit>()
            val rows = mutableListOf(deliveryTestMessage())
            // The first value waits until the state callback has thrown.
            val reader =
                RecordingMessageReader {
                    stateCalled.await()
                    delay(100)
                    rows.removeFirstOrNull()
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            val uncaught = AtomicReference<Throwable?>()
            val previous = Thread.getDefaultUncaughtExceptionHandler()
            Thread.setDefaultUncaughtExceptionHandler { _, error -> uncaught.compareAndSet(null, error) }
            try {
                val delivered =
                    client
                        .messages(onConnectionStateChange = { _, _ ->
                            stateCalled.complete(Unit)
                            throw IllegalStateException("state callback failed")
                        })
                        .toList()
                assertEquals(listOf("01".repeat(32)), delivered.map { it.id })
                assertNull("A state callback error crashed its coroutine", uncaught.get())
            } finally {
                Thread.setDefaultUncaughtExceptionHandler(previous)
            }
        }
}
