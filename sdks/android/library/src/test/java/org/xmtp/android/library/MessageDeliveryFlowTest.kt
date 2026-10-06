package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.util.concurrent.atomic.AtomicLong

private const val DELIVERY_FLOW_TEST_TIMEOUT_MS = 10_000L
private val readerTestKeys = AtomicLong(1)

internal fun deliveryTestMessage(
    content: MessageContent = MessageContent.Text("message"),
    rawBytes: ByteArray = byteArrayOf(1),
    fallback: String? = null,
    encoded: EncodedContent? = null,
    id: String = "01".repeat(32),
    clientKey: ULong = 1uL,
): Message =
    Message(
        MessageData(
            id = id,
            clientKey = clientKey,
            deliveryCursor = "cursor-$id",
            conversationId = "02".repeat(16),
            topic = "test-topic",
            senderInboxId = "03".repeat(32),
            sentAt = Timestamp(1),
            insertedAt = Timestamp(2),
            expiresAt = null,
            kind = MessageKind.APPLICATION,
            deliveryStatus = DeliveryStatus.PUBLISHED,
            rawBytes = rawBytes,
            contentType = encoded?.type,
            fallback = fallback,
            encoded = encoded,
            content = content,
            replyCount = 0uL,
            reactions = emptyList(),
            inReplyTo = null,
        ),
    )

// This boundary records host calls. It does not model native ACK or ownership.
internal class RecordingMessageReader(
    private val read: suspend () -> Message?,
) : MessageReader(NoHandle) {
    var nextCalls = 0
    var endCalls = 0

    override suspend fun next(): Message? {
        nextCalls++
        return read()
    }

    override suspend fun end() {
        endCalls++
    }

    override suspend fun connectionState() = ConnectionState.CONNECTED

    override suspend fun connectionStateChanged(previous: ConnectionState): ConnectionState = awaitCancellation()
}

internal class RecordingReaderClient(
    private val open: suspend () -> MessageReader,
) : Client(NoHandle) {
    val key = readerTestKeys.getAndIncrement().toULong()
    var closed = false
    val readerOptions = mutableListOf<MessageReaderOptions?>()
    private val conversations =
        object : Conversations(NoHandle) {
            override fun sdkStreamOwnerKey(): ULong = key

            override suspend fun messageReader(options: MessageReaderOptions?): MessageReader {
                readerOptions.add(options)
                return open()
            }
        }

    override fun clientKey(): ULong {
        if (closed) {
            throw XmtpException.ClientClosed(
                ErrorDetails("ClientClosed", ErrorCategory.LIFECYCLE, false, "client ended"),
            )
        }
        return key
    }

    override fun conversations(): Conversations = conversations
}

internal fun streamFailure(code: String = "Storage"): XmtpException =
    XmtpException.Storage(
        ErrorDetails(code, ErrorCategory.STORAGE, true, "native acknowledgement failed"),
    )

class MessageDeliveryFlowTest {
    // The conversations.streamAllMessages(options) method must hand the
    // caller's filter and cursor to the native reader unchanged.
    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun messagesPassesTheReaderOptionsToTheNativeReader() =
        runBlocking {
            val options =
                MessageStreamOptions(
                    conversationKind = ConversationKind.GROUP,
                    consentStates = listOf(ConsentState.ALLOWED),
                    from = "cursor-from",
                )
            val raw = RecordingReaderClient { RecordingMessageReader { null } }
            val client = testSDKClient(raw)
            client.conversations.streamAllMessages(options = options).collect()
            assertEquals(
                listOf<MessageReaderOptions?>(
                    MessageReaderOptions(
                        conversationKind = ConversationKind.GROUP,
                        consentStates = listOf(ConsentState.ALLOWED),
                        from = "cursor-from",
                    ),
                ),
                raw.readerOptions,
            )
            client.conversations.streamAllMessages().collect()
            assertEquals(MessageReaderOptions(), raw.readerOptions.last())
        }

    // Host sequencing only. Native final commit admission has separate core proof.
    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun acknowledgesOnlyAfterTheDirectCollectorReturnsAndClosesOnce() =
        runBlocking {
            val delivered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val first = deliveryTestMessage()
            var remaining = first
            var hasMessage = true
            val reader =
                RecordingMessageReader {
                    if (hasMessage) {
                        hasMessage = false
                        remaining
                    } else {
                        null
                    }
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            val received = mutableListOf<Message>()
            val closes = mutableListOf<SDKStreamCloseReason>()
            val job =
                launch {
                    client.conversations
                        .streamAllMessages(
                            options = MessageStreamOptions(onClose = { closes.add(it) }),
                        ).collect {
                            received.add(it)
                            assertEquals(1, reader.nextCalls)
                            delivered.complete(Unit)
                            release.await()
                        }
                }
            delivered.await()
            assertEquals(1, reader.nextCalls)
            release.complete(Unit)
            job.join()
            assertEquals(listOf(first), received)
            assertEquals(2, reader.nextCalls)
            assertEquals(1, reader.endCalls)
            assertEquals(listOf(SDKStreamCloseReason.Closed), closes)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun cancellationRejectsTheCurrentItemAndTheQueuedItem() =
        runBlocking {
            val entered = CompletableDeferred<Unit>()
            val rows = mutableListOf(deliveryTestMessage(), deliveryTestMessage(id = "04".repeat(32)))
            val reader = RecordingMessageReader { rows.removeFirstOrNull() }
            val client = testSDKClient(RecordingReaderClient { reader })
            var received = 0
            val closes = mutableListOf<SDKStreamCloseReason>()
            val job =
                launch {
                    client.conversations
                        .streamAllMessages(
                            options = MessageStreamOptions(onClose = { closes.add(it) }),
                        ).collect {
                            received++
                            entered.complete(Unit)
                            awaitCancellation()
                        }
                }
            entered.await()
            job.cancelAndJoin()
            assertEquals(1, received)
            assertEquals(1, reader.nextCalls)
            assertEquals(1, rows.size)
            assertEquals(1, reader.endCalls)
            assertEquals(listOf(SDKStreamCloseReason.Closed), closes)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun ownershipFailureClosesWithoutAnotherRead() =
        runBlocking {
            val error =
                XmtpException.ConsumerOwned(
                    ErrorDetails("ConsumerOwned", ErrorCategory.STREAM, false, "ownership lost"),
                )
            val reader = RecordingMessageReader { throw error }
            val client = testSDKClient(RecordingReaderClient { reader })
            var received = 0
            val closes = mutableListOf<SDKStreamCloseReason>()
            val failure =
                runCatching {
                    client.conversations
                        .streamAllMessages(
                            options =
                                MessageStreamOptions(onClose = {
                                    closes.add(it)
                                }),
                        ).collect { received++ }
                }.exceptionOrNull()
            assertSame(error, failure)
            assertEquals(0, received)
            assertEquals(1, reader.nextCalls)
            assertEquals(1, reader.endCalls)
            assertSame(error, (closes.single() as SDKStreamCloseReason.Failed).error)
        }

    // A native read failure keeps its Throwable and closes once with Failed,
    // for an Exception and for an Error (moved from the deleted Kotlin
    // conformance RetainedReaders.kt checkReaderReadFailuresEndExactlyOnce).
    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun readFailureOfEachThrowableKindClosesOnceWithFailed() =
        runBlocking {
            for (failure in listOf(IllegalStateException("read failed"), AssertionError("native read failed"))) {
                val reader = RecordingMessageReader { throw failure }
                val client = testSDKClient(RecordingReaderClient { reader })
                val closes = mutableListOf<SDKStreamCloseReason>()
                var received = 0
                assertSame(
                    failure,
                    runCatching {
                        client.conversations
                            .streamAllMessages(
                                MessageStreamOptions(onClose = { closes.add(it) }),
                            ).collect { received++ }
                    }.exceptionOrNull(),
                )
                assertEquals(0, received)
                assertEquals(1, reader.nextCalls)
                assertEquals(1, reader.endCalls)
                assertSame(failure, (closes.single() as SDKStreamCloseReason.Failed).error)
            }
        }

    // verifies: PROC-041
    // A collector failure closes once with Closed, separate from native reader failures.
    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun collectorExceptionsPropagateWithClosedOnce() =
        runBlocking {
            for (failure in listOf(IllegalStateException("collector failed"), AssertionError("collector failed"))) {
                val reader = RecordingMessageReader { deliveryTestMessage() }
                val client = testSDKClient(RecordingReaderClient { reader })
                val closes = mutableListOf<SDKStreamCloseReason>()
                assertSame(
                    failure,
                    runCatching {
                        client.conversations
                            .streamAllMessages(
                                options =
                                    MessageStreamOptions(onClose = {
                                        closes.add(it)
                                    }),
                            ).collect { throw failure }
                    }.exceptionOrNull(),
                )
                assertEquals(1, reader.nextCalls)
                assertEquals(1, reader.endCalls)
                assertEquals(listOf(SDKStreamCloseReason.Closed), closes)
            }
        }
}
