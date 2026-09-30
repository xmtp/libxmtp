package org.xmtp.android.library

import com.google.protobuf.InvalidProtocolBufferException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.DecodedMessage
import uniffi.xmtpv3.FfiDeliveryCursor
import uniffi.xmtpv3.FfiHistoryMessage
import uniffi.xmtpv3.FfiMessageHistorySnapshot

private const val MESSAGE_READER_TEST_TIMEOUT_MS = 10_000L

class MessageReaderTest {
    private class Delivery(
        val value: Int? = 1,
        val current: Boolean = true,
        val acknowledgementError: Throwable? = null,
        val onCheck: () -> Unit = {},
        val decodeValue: () -> Int? = { value },
    ) {
        var acknowledgements = 0
        var rejections = 0

        fun queued(): QueuedMessageDelivery<Int> =
            QueuedMessageDelivery(
                decode = decodeValue,
                checkOwner = {
                    onCheck()
                    current
                },
                acknowledge = {
                    acknowledgementError?.let { throw it }
                    acknowledgements++
                },
                reject = { rejections++ },
            )
    }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun nextAcknowledgesOnlyThePreviousItemAndCloseRejectsTheLast() =
        runBlocking {
            for (value in listOf(1, null)) {
                val first = Delivery(value)
                val second = Delivery(2)
                val items = mutableListOf(first.queued(), second.queued())
                var ended = 0
                val reader =
                    AcknowledgedMessageReader(
                        read = { items.removeAt(0) },
                        end = { ended++ },
                    )
                if (value != null) {
                    assertEquals(1, reader.next())
                    assertEquals(0, first.acknowledgements)
                }
                assertEquals(2, reader.next())
                assertEquals(1, first.acknowledgements)
                assertEquals(0, second.acknowledgements)
                reader.close()
                reader.close()
                assertNull(reader.next())
                assertEquals(0, first.rejections)
                assertEquals(1, second.rejections)
                assertEquals(1, ended)
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun cancellationWhileWaitingClosesTheReader() =
        runBlocking {
            val started = CompletableDeferred<Unit>()
            val incoming = CompletableDeferred<QueuedMessageDelivery<Int>?>()
            var ended = 0
            val reader =
                AcknowledgedMessageReader(
                    read = {
                        started.complete(Unit)
                        incoming.await()
                    },
                    end = { ended++ },
                )
            val task = launch { reader.next() }
            started.await()
            task.cancelAndJoin()
            assertEquals(1, ended)
            assertNull(reader.next())
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun cancellationBeforeHandoffRejectsTheCurrentToken() =
        runBlocking {
            for (value in listOf(1, null)) {
                var ended = 0
                lateinit var item: Delivery
                val task =
                    launch {
                        val context = coroutineContext
                        item = Delivery(value, onCheck = { context.cancel() })
                        val reader =
                            AcknowledgedMessageReader(
                                read = { item.queued() },
                                end = { ended++ },
                            )
                        reader.next()
                    }
                task.join()
                assertEquals(1, item.rejections)
                assertEquals(0, item.acknowledgements)
                assertEquals(1, ended)
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun selectionChangeDiscardsWithoutAcknowledgingAndReadsFreshSelection() =
        runBlocking {
            for (value in listOf(1, null)) {
                val stale = Delivery(value, current = false)
                val fresh = Delivery(2)
                val items = mutableListOf(stale.queued(), fresh.queued())
                val reader = AcknowledgedMessageReader(read = { items.removeAt(0) }, end = {})
                assertEquals(2, reader.next())
                assertEquals(1, stale.rejections)
                assertEquals(0, stale.acknowledgements)
                assertEquals(0, fresh.acknowledgements)
                reader.close()
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun acknowledgementFailureRetainsTheTokenForExplicitRejection() =
        runBlocking {
            for (value in listOf(1, null)) {
                val error = IllegalStateException("acknowledgement failed")
                val first = Delivery(value, acknowledgementError = error)
                var reads = 0
                var ended = 0
                val reader =
                    AcknowledgedMessageReader(
                        read = {
                            reads++
                            first.queued()
                        },
                        end = { ended++ },
                    )
                if (value != null) assertEquals(1, reader.next())
                assertSame(error, runCatching { reader.next() }.exceptionOrNull())
                assertEquals(1, reads)
                assertEquals(1, first.rejections)
                assertEquals(0, first.acknowledgements)
                assertEquals(1, ended)
                assertNull(reader.next())
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun decodeFailureClosesWithoutAcknowledgement() =
        runBlocking {
            val validCursor = FfiDeliveryCursor(databaseId = ByteArray(16), deliverySequence = 1uL)
            val snapshotCursor = FfiDeliveryCursor(databaseId = ByteArray(16), deliverySequence = 2uL)
            val invalidEncoding =
                TextCodec()
                    .encode("hi")
                    .toBuilder()
                    .putParameters("encoding", "UTF-16")
                    .build()
                    .toByteArray()
            for ((content, errorType) in listOf(
                byteArrayOf(0x80.toByte()) to InvalidProtocolBufferException::class.java,
                invalidEncoding to XMTPException::class.java,
            )) {
                val message = deliveryTestMessage(content)
                val invalid = Delivery(decodeValue = { DecodedMessage.createForDelivery(message, null)?.let { 1 } })
                var reads = 0
                var ended = 0
                val invalidReader =
                    AcknowledgedMessageReader(
                        read = { if (reads++ == 0) invalid.queued() else null },
                        end = { ended++ },
                    )
                val failure = runCatching { invalidReader.next() }.exceptionOrNull()
                assertEquals(errorType, failure?.javaClass)
                assertEquals(1, reads)
                assertEquals(0, invalid.acknowledgements)
                assertEquals(1, invalid.rejections)
                assertEquals(1, ended)
                assertNull(invalidReader.next())

                val snapshot =
                    FfiMessageHistorySnapshot(
                        messages = listOf(FfiHistoryMessage(message = message, cursor = snapshotCursor)),
                        cursor = snapshotCursor,
                    )
                val snapshotFailure = runCatching { snapshot.toMessageHistorySnapshot() }.exceptionOrNull()
                assertEquals(errorType, snapshotFailure?.javaClass)
            }

            val validMessage = deliveryTestMessage(TextCodec().encode("hi").toByteArray())
            val filteredMessage =
                deliveryTestMessage(
                    EncodedContent
                        .newBuilder()
                        .setType(ContentTypeGroupUpdated)
                        .build()
                        .toByteArray(),
                )
            assertNull(DecodedMessage.createForDelivery(filteredMessage, null))
            val snapshot =
                FfiMessageHistorySnapshot(
                    messages =
                        listOf(
                            FfiHistoryMessage(message = validMessage, cursor = validCursor),
                            FfiHistoryMessage(message = filteredMessage, cursor = snapshotCursor),
                        ),
                    cursor = snapshotCursor,
                ).toMessageHistorySnapshot()
            assertEquals(listOf("hi"), snapshot.messages.map { it.body })
            val deliveredCursor = requireNotNull(snapshot.messages.single().deliveryCursor)
            assertArrayEquals(validCursor.databaseId, deliveredCursor.databaseId)
            assertEquals(validCursor.deliverySequence, deliveredCursor.deliverySequence)
            assertArrayEquals(snapshotCursor.databaseId, snapshot.cursor.databaseId)
            assertEquals(snapshotCursor.deliverySequence, snapshot.cursor.deliverySequence)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun closeDuringOwnershipCheckPreventsHandoff() =
        runBlocking {
            for (value in listOf(1, null)) {
                lateinit var reader: AcknowledgedMessageReader<Int>
                var ended = 0
                val item = Delivery(value, onCheck = { reader.close() })
                reader = AcknowledgedMessageReader(read = { item.queued() }, end = { ended++ })
                assertNull(reader.next())
                assertEquals(1, item.rejections)
                assertEquals(0, item.acknowledgements)
                assertEquals(1, ended)
            }
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun concurrentNextClosesBothRequests() =
        runBlocking {
            val started = CompletableDeferred<Unit>()
            val incoming = CompletableDeferred<QueuedMessageDelivery<Int>?>()
            var ended = 0
            val reader =
                AcknowledgedMessageReader(
                    read = {
                        started.complete(Unit)
                        incoming.await()
                    },
                    end = {
                        ended++
                        incoming.complete(null)
                        Unit
                    },
                )
            val first = launch { assertNull(reader.next()) }
            started.await()
            assertTrue(runCatching { reader.next() }.exceptionOrNull() is XMTPException)
            first.join()
            assertEquals(1, ended)
        }
}
