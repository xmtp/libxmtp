package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

private const val MESSAGE_READER_TEST_TIMEOUT_MS = 10_000L

class MessageReaderTest {
    // This checks host call order. Core tests own durable ACK and replay.
    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun flowRequestsNextOnlyAfterThePreviousCollectorReturns() =
        runBlocking {
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            var items = 0
            val reader =
                RecordingMessageReader {
                    when (items++) {
                        0 -> deliveryTestMessage()
                        1 -> deliveryTestMessage(id = "07".repeat(32))
                        else -> null
                    }
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            val received = mutableListOf<String>()
            val job =
                launch {
                    client.messages().collect {
                        received.add(it.id)
                        if (received.size == 1) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                }
            entered.await()
            assertEquals(1, reader.nextCalls)
            release.complete(Unit)
            job.join()
            assertEquals(listOf("01".repeat(32), "07".repeat(32)), received)
            assertEquals(3, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun cancellationWhileWaitingClosesTheReader() =
        runBlocking {
            val started = CompletableDeferred<Unit>()
            val reader =
                RecordingMessageReader {
                    started.complete(Unit)
                    awaitCancellation()
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            val job = launch { client.messages().collect { fail("Waiting read cannot emit") } }
            started.await()
            job.cancelAndJoin()
            assertEquals(1, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun cancellationBeforeHandoffEndsWithoutEmission() =
        runBlocking {
            val reader =
                RecordingMessageReader {
                    currentCoroutineContext().cancel()
                    deliveryTestMessage()
                }
            val client = testSDKClient(RecordingReaderClient { reader })
            var delivered = 0
            val job = launch { client.messages().collect { delivered++ } }
            job.join()
            assertEquals(0, delivered)
            assertEquals(1, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun ownershipFailureClosesWithoutAnotherRead() =
        runBlocking {
            val failure =
                XmtpException.ConsumerOwned(
                    ErrorDetails("ConsumerOwned", ErrorCategory.STREAM, false, "selection owner changed"),
                )
            val reader = RecordingMessageReader { throw failure }
            val client = testSDKClient(RecordingReaderClient { reader })
            assertSame(
                failure,
                runCatching {
                    client.messages().collect { fail("Ownership failure cannot emit") }
                }.exceptionOrNull(),
            )
            assertEquals(1, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun acknowledgementFailureClosesBeforeAnotherHandoff() =
        runBlocking {
            val failure = streamFailure()
            var reads = 0
            val reader = RecordingMessageReader { if (reads++ == 0) deliveryTestMessage() else throw failure }
            val client = testSDKClient(RecordingReaderClient { reader })
            var delivered = 0
            assertSame(failure, runCatching { client.messages().collect { delivered++ } }.exceptionOrNull())
            assertEquals(1, delivered)
            assertEquals(2, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun decodeFailureIsHandedOffAndReaderContinues() =
        runBlocking {
            for (raw in listOf(byteArrayOf(0x80.toByte()), byteArrayOf(1, 2, 3))) {
                val details = ErrorDetails("MalformedEnvelope", ErrorCategory.INPUT, false, "malformed")
                val unknown = deliveryTestMessage(MessageContent.Unknown(null, raw, details), raw)
                val later = deliveryTestMessage(MessageContent.Text("later"), id = "08".repeat(32))
                val rows = mutableListOf(unknown, later)
                val reader = RecordingMessageReader { rows.removeFirstOrNull() }
                val client = testSDKClient(RecordingReaderClient { reader })
                val delivered = mutableListOf<Message>()
                client.messages().collect { delivered.add(it) }
                assertEquals(listOf(unknown.id, later.id), delivered.map { it.id })
                val content = delivered.first().content as SDKMessageContent.Unknown
                assertArrayEquals(raw, content.rawBytes)
                assertArrayEquals(raw, delivered.first().rawBytes)
                assertEquals(details, content.error)
                assertEquals(unknown.deliveryCursor, delivered.first().deliveryCursor)
                assertNull(content.encoded)
                assertEquals("later", (delivered.last().data.content as MessageContent.Text).v1)
                assertEquals(1, reader.endCalls)
            }
        }

    // verifies: PROC-045
    // A failed custom decode reaches the collector with its error, and the
    // same stream delivers the next message.
    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun codecCancellationIsContainedAndReaderContinues() =
        assertCodecFailureIsContained(CancellationException("codec cancelled"))

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun codecLinkageErrorIsContainedAndReaderContinues() =
        assertCodecFailureIsContained(LinkageError("codec dependency missing"))

    private fun assertCodecFailureIsContained(failure: Throwable) =
        runBlocking {
            val type = ContentTypeId("example.com", "control-failure-test", 1u, 0u)
            val codec =
                object : ContentCodec<String> {
                    override val type = type

                    override fun encode(value: String) = EncodedContent(type, content = value.toByteArray())

                    override fun decode(encoded: EncodedContent): String = throw failure
                }
            var reads = 0
            lateinit var rawClient: RecordingReaderClient
            val encoded = EncodedContent(type, content = byteArrayOf(1))
            val reader =
                RecordingMessageReader {
                    when (reads++) {
                        0 -> {
                            deliveryTestMessage(
                                MessageContent.Custom(encoded, byteArrayOf(2)),
                                byteArrayOf(2),
                                encoded = encoded,
                                clientKey = rawClient.key,
                            )
                        }

                        1 -> {
                            deliveryTestMessage(
                                MessageContent.Text("later"),
                                id = "09".repeat(32),
                                clientKey = rawClient.key,
                            )
                        }

                        else -> {
                            null
                        }
                    }
                }
            rawClient = RecordingReaderClient { reader }
            val client = testSDKClient(rawClient, listOf(codec))
            ClientRegistry.register(client)
            try {
                val received = mutableListOf<Message>()
                client.messages().collect { received.add(it) }
                assertEquals(2, received.size)
                val failed = received.first().content as SDKMessageContent.Custom
                assertNull(failed.value)
                assertEquals("CodecDecodeFailed", failed.error?.code)
                assertEquals(ErrorCategory.CALLBACK, failed.error?.category)
                assertFalse(checkNotNull(failed.error).retryable)
                assertTrue(checkNotNull(failed.error).message.contains(checkNotNull(failure.message)))
                assertEquals(encoded, failed.encoded)
                assertArrayEquals(byteArrayOf(2), failed.rawBytes)
                assertEquals("later", (received.last().data.content as MessageContent.Text).v1)
                assertEquals(1, reader.endCalls)
            } finally {
                ClientRegistry.remove(client)
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun clientCloseBeforeReadPreventsNativeRead() =
        runBlocking {
            val reader = RecordingMessageReader { deliveryTestMessage() }
            val raw = RecordingReaderClient { reader }
            val client = testSDKClient(raw)
            raw.closed = true
            assertTrue(
                runCatching {
                    client.messages().collect { fail("Closed owner cannot emit") }
                }.exceptionOrNull() is XmtpException.ClientClosed,
            )
            assertEquals(0, reader.nextCalls)
            assertEquals(1, reader.endCalls)
        }

    // Each host collection owns its reader. Native competing leases remain core proof.
    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun independentCollectionsOwnSeparateReaders() =
        runBlocking {
            val opened = CompletableDeferred<Unit>()
            val readers = mutableListOf<RecordingMessageReader>()
            val client =
                testSDKClient(
                    RecordingReaderClient {
                        RecordingMessageReader { awaitCancellation() }.also {
                            synchronized(readers) {
                                readers.add(it)
                                if (readers.size == 2) opened.complete(Unit)
                            }
                        }
                    },
                )
            val first = launch { client.messages().collect {} }
            val second = launch { client.messages().collect {} }
            opened.await()
            first.cancelAndJoin()
            second.cancelAndJoin()
            assertEquals(2, readers.size)
            assertNotSame(readers[0], readers[1])
            assertEquals(listOf(1, 1), readers.map { it.endCalls })
        }

    // verifies: CTYPE-008, CTYPE-009
    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun unknownContentKeepsEvidenceAndTypedFailure() {
        val raw = byteArrayOf(0x80.toByte())
        val failure = ErrorDetails("MalformedEnvelope", ErrorCategory.INPUT, false, "malformed")
        val message = deliveryTestMessage(MessageContent.Unknown(null, raw, failure), raw)
        val content = message.content as SDKMessageContent.Unknown
        assertArrayEquals(raw, content.rawBytes)
        assertArrayEquals(raw, message.rawBytes)
        assertNull(content.encoded)
        assertEquals("MalformedEnvelope", content.error.code)
        assertEquals(ErrorCategory.INPUT, content.error.category)
        assertEquals("malformed", content.error.message)
        assertFalse(content.error.retryable)
        assertEquals("cursor-" + message.id, message.deliveryCursor)
    }
}
