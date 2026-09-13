package org.xmtp.android.library

import com.google.protobuf.InvalidProtocolBufferException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.libxmtp.DecodedMessage
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiException
import uniffi.xmtpv3.FfiMessage

private const val DELIVERY_FLOW_TEST_TIMEOUT_MS = 10_000L

internal fun deliveryTestMessage(
    content: ByteArray,
    kind: FfiConversationMessageKind = FfiConversationMessageKind.APPLICATION,
): FfiMessage =
    FfiMessage(
        id = ByteArray(32) { 1 },
        sentAtNs = 1L,
        conversationId = ByteArray(16) { 2 },
        senderInboxId = "sender",
        content = content,
        kind = kind,
        deliveryStatus = FfiDeliveryStatus.PUBLISHED,
        sequenceId = 1UL,
        insertedAtNs = 1L,
        expireAtNs = null,
    )

class MessageDeliveryFlowTest {
    private class Delivery(
        val value: Int? = 1,
        var current: Boolean = true,
        val acknowledgeError: Throwable? = null,
        val decodeValue: (() -> Int?)? = null,
        val onCheck: () -> Unit = {},
    ) {
        var acknowledgements = 0
        var rejections = 0
        val acknowledged = CompletableDeferred<Unit>()
        val rejected = CompletableDeferred<Unit>()

        fun queued(): QueuedMessageDelivery<Int> =
            QueuedMessageDelivery(
                decode = {
                    if (decodeValue != null) decodeValue() else value
                },
                checkOwner = {
                    onCheck()
                    current
                },
                acknowledge = {
                    acknowledgeError?.let { throw it }
                    acknowledgements++
                    acknowledged.complete(Unit)
                    Unit
                },
                reject = {
                    rejections++
                    rejected.complete(Unit)
                    Unit
                },
            )
    }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun acknowledgesOnlyAfterTheDirectCollectorReturnsAndClosesOnce() =
        runBlocking {
            val ready = CompletableDeferred<MessageDeliveryCallback<Int>>()
            val handedOff = CompletableDeferred<Unit>()
            val releaseCollector = CompletableDeferred<Unit>()
            val delivery = Delivery()
            val reservedContent =
                EncodedContent
                    .newBuilder()
                    .setType(ContentTypeGroupUpdated)
                    .build()
                    .toByteArray()
            val forgedMessage = deliveryTestMessage(reservedContent)
            assertNotNull(
                DecodedMessage.createForDelivery(
                    forgedMessage.copy(kind = FfiConversationMessageKind.MEMBERSHIP_CHANGE),
                    null,
                ),
            )
            val filtered =
                Delivery(decodeValue = { DecodedMessage.createForDelivery(forgedMessage, null)?.let { 99 } })
            val received = mutableListOf<Int>()
            var ended = 0
            var closed = 0
            val job =
                launch {
                    acknowledgedMessageFlow<Int>({ closed++ }) { callback ->
                        ready.complete(callback)
                        return@acknowledgedMessageFlow { ended++ }
                    }.collect {
                        received.add(it)
                        assertEquals(0, delivery.acknowledgements)
                        handedOff.complete(Unit)
                        releaseCollector.await()
                    }
                }
            val callback = ready.await()
            callback.onMessage(filtered.queued())
            filtered.acknowledged.await()
            assertEquals(1, filtered.acknowledgements)
            assertEquals(0, filtered.rejections)
            assertTrue(received.isEmpty())
            callback.onMessage(delivery.queued())
            handedOff.await()
            assertEquals(0, delivery.acknowledgements)
            releaseCollector.complete(Unit)
            delivery.acknowledged.await()
            callback.onClose()
            callback.onClose()
            job.join()
            assertEquals(1, delivery.acknowledgements)
            assertEquals(0, delivery.rejections)
            assertEquals(listOf(1), received)
            assertEquals(1, ended)
            assertEquals(1, closed)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun cancellationRejectsTheCurrentItemAndTheQueuedItem() =
        runBlocking {
            for (filterCurrent in listOf(false, true)) {
                val ready = CompletableDeferred<MessageDeliveryCallback<Int>>()
                val handedOff = CompletableDeferred<Unit>()
                val releaseCollector = CompletableDeferred<Unit>()
                val queued = Delivery(2)
                lateinit var callback: MessageDeliveryCallback<Int>
                lateinit var job: Job
                val current =
                    Delivery(
                        value = if (filterCurrent) null else 1,
                        onCheck = {
                            if (filterCurrent) {
                                callback.onMessage(queued.queued())
                                job.cancel()
                            }
                        },
                    )
                val received = mutableListOf<Int>()
                job =
                    launch {
                        acknowledgedMessageFlow<Int>(null) { registered ->
                            ready.complete(registered)
                            return@acknowledgedMessageFlow {}
                        }.collect {
                            received.add(it)
                            handedOff.complete(Unit)
                            releaseCollector.await()
                        }
                    }
                callback = ready.await()
                callback.onMessage(current.queued())
                if (!filterCurrent) {
                    handedOff.await()
                    callback.onMessage(queued.queued())
                    job.cancelAndJoin()
                } else {
                    job.join()
                }
                assertEquals(if (filterCurrent) emptyList<Int>() else listOf(1), received)
                assertEquals(0, current.acknowledgements)
                assertEquals(0, queued.acknowledgements)
                assertEquals(1, current.rejections)
                assertEquals(1, queued.rejections)
            }
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun selectionChangeReselectsWithoutAcknowledgement() =
        runBlocking {
            for (value in listOf(1, null)) {
                val ready = CompletableDeferred<MessageDeliveryCallback<Int>>()
                val stale = Delivery(value = value, current = false)
                val current = Delivery(2)
                val received = mutableListOf<Int>()
                val job =
                    launch {
                        acknowledgedMessageFlow<Int>(null) { callback ->
                            ready.complete(callback)
                            return@acknowledgedMessageFlow {}
                        }.collect { received.add(it) }
                    }
                val callback = ready.await()
                callback.onMessage(stale.queued())
                stale.rejected.await()
                callback.onMessage(current.queued())
                current.acknowledged.await()
                callback.onClose()
                job.join()
                assertEquals(listOf(2), received)
                assertEquals(0, stale.acknowledgements)
                assertEquals(1, stale.rejections)
            }
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun decodeAndCollectorFailuresRejectTheItem() =
        runBlocking {
            val error = IllegalStateException("collector failed")
            val pending = Delivery()
            val failed =
                runCatching {
                    acknowledgedMessageFlow<Int>(null) { callback ->
                        callback.onMessage(pending.queued())
                        callback.onClose()
                        return@acknowledgedMessageFlow {}
                    }.collect { throw error }
                }
            assertSame(error, failed.exceptionOrNull())
            assertEquals(0, pending.acknowledgements)
            assertEquals(1, pending.rejections)

            val invalidEncoding =
                TextCodec()
                    .encode("hi")
                    .toBuilder()
                    .putParameters("encoding", "UTF-16")
                    .build()
            for (
            (content, errorType) in
            listOf(
                byteArrayOf(0x80.toByte()) to InvalidProtocolBufferException::class.java,
                invalidEncoding.toByteArray() to XMTPException::class.java,
            )
            ) {
                val delivery =
                    Delivery(
                        decodeValue = {
                            DecodedMessage.createForDelivery(deliveryTestMessage(content), null)?.let { 1 }
                        },
                    )
                val received = mutableListOf<Int>()
                val result =
                    runCatching {
                        acknowledgedMessageFlow<Int>(null) { callback ->
                            callback.onMessage(delivery.queued())
                            callback.onClose()
                            return@acknowledgedMessageFlow {}
                        }.collect { received.add(it) }
                    }
                assertTrue(errorType.isInstance(result.exceptionOrNull()))
                assertTrue(received.isEmpty())
                assertEquals(0, delivery.acknowledgements)
                assertEquals(1, delivery.rejections)
            }
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun acknowledgementFailureStopsBeforeTheNextHandoff() =
        runBlocking {
            for (value in listOf(1, null)) {
                val error = IllegalStateException("acknowledgement failed")
                val second = Delivery(2)
                lateinit var callback: MessageDeliveryCallback<Int>
                val first =
                    Delivery(
                        value = value,
                        acknowledgeError = error,
                        onCheck = { callback.onMessage(second.queued()) },
                    )
                val received = mutableListOf<Int>()
                val result =
                    runCatching {
                        acknowledgedMessageFlow<Int>(null) {
                            callback = it
                            it.onMessage(first.queued())
                            return@acknowledgedMessageFlow {}
                        }.collect { received.add(it) }
                    }
                assertSame(error, result.exceptionOrNull())
                assertEquals(listOfNotNull(value), received)
                assertEquals(0, first.acknowledgements)
                assertEquals(0, second.acknowledgements)
                assertEquals(1, first.rejections)
                assertEquals(1, second.rejections)
            }
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun fullQueueRejectsBothItemsWithoutHandoff() =
        runBlocking {
            val first = Delivery()
            val second = Delivery(2)
            val received = mutableListOf<Int>()
            val result =
                runCatching {
                    acknowledgedMessageFlow<Int>(null) { callback ->
                        callback.onMessage(first.queued())
                        callback.onMessage(second.queued())
                        return@acknowledgedMessageFlow {}
                    }.collect { received.add(it) }
                }
            assertTrue(result.exceptionOrNull() is XMTPException)
            assertTrue(received.isEmpty())
            assertEquals(1, first.rejections)
            assertEquals(1, second.rejections)
            assertEquals(0, first.acknowledgements)
            assertEquals(0, second.acknowledgements)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun nativeErrorsReachTheCollector() =
        runBlocking {
            val error = FfiException.Exception("native failure")
            val result =
                runCatching {
                    acknowledgedMessageFlow<Int>(null) { callback ->
                        callback.onError(error)
                        return@acknowledgedMessageFlow {}
                    }.collect {}
                }
            val received = result.exceptionOrNull()
            assertTrue(received is FfiException.Exception)
            assertEquals(error.message, received?.message)
        }
}
