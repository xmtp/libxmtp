package org.xmtp.android.library

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.xmtpv3.FfiException

private const val DELIVERY_FLOW_TEST_TIMEOUT_MS = 10_000L

class MessageDeliveryFlowTest {
    private class Delivery(
        val value: Int = 1,
        var current: Boolean = true,
        val decodeError: Throwable? = null,
        val acknowledgeError: Throwable? = null,
    ) {
        var checks = 0
        var acknowledgements = 0
        var rejections = 0
        val acknowledged = CompletableDeferred<Unit>()
        val rejected = CompletableDeferred<Unit>()

        fun queued(): QueuedMessageDelivery<Int> =
            QueuedMessageDelivery(
                decode = {
                    decodeError?.let { throw it }
                    value
                },
                checkOwner = {
                    checks++
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
            var ended = 0
            var closed = 0
            val job =
                launch {
                    acknowledgedMessageFlow<Int>({ closed++ }) { callback ->
                        ready.complete(callback)
                        return@acknowledgedMessageFlow { ended++ }
                    }.collect {
                        assertEquals(1, delivery.checks)
                        assertEquals(0, delivery.acknowledgements)
                        handedOff.complete(Unit)
                        releaseCollector.await()
                    }
                }
            val callback = ready.await()
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
            assertEquals(1, ended)
            assertEquals(1, closed)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun cancellationRejectsTheCurrentItemAndTheQueuedItem() =
        runBlocking {
            val ready = CompletableDeferred<MessageDeliveryCallback<Int>>()
            val handedOff = CompletableDeferred<Unit>()
            val releaseCollector = CompletableDeferred<Unit>()
            val current = Delivery()
            val queued = Delivery(2)
            val job =
                launch {
                    acknowledgedMessageFlow<Int>(null) { callback ->
                        ready.complete(callback)
                        return@acknowledgedMessageFlow {}
                    }.collect {
                        handedOff.complete(Unit)
                        releaseCollector.await()
                    }
                }
            val callback = ready.await()
            callback.onMessage(current.queued())
            handedOff.await()
            callback.onMessage(queued.queued())
            job.cancelAndJoin()
            assertEquals(0, current.acknowledgements)
            assertEquals(0, queued.acknowledgements)
            assertEquals(1, current.rejections)
            assertEquals(1, queued.rejections)
            assertEquals(0, queued.checks)
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun selectionChangeReselectsWithoutAcknowledgement() =
        runBlocking {
            val ready = CompletableDeferred<MessageDeliveryCallback<Int>>()
            val stale = Delivery(current = false)
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

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun decodeAndCollectorFailuresRejectTheItem() =
        runBlocking {
            for (decodeFails in listOf(true, false)) {
                val error = IllegalStateException("test failure")
                val delivery = Delivery(decodeError = error.takeIf { decodeFails })
                val result =
                    runCatching {
                        acknowledgedMessageFlow<Int>(null) { callback ->
                            callback.onMessage(delivery.queued())
                            callback.onClose()
                            return@acknowledgedMessageFlow {}
                        }.collect { throw error }
                    }
                assertSame(error, result.exceptionOrNull())
                assertEquals(0, delivery.acknowledgements)
                assertEquals(1, delivery.rejections)
            }
        }

    @Test(timeout = DELIVERY_FLOW_TEST_TIMEOUT_MS)
    fun acknowledgementFailureStopsBeforeTheNextHandoff() =
        runBlocking {
            val error = IllegalStateException("acknowledgement failed")
            val first = Delivery(acknowledgeError = error)
            val second = Delivery(2)
            lateinit var callback: MessageDeliveryCallback<Int>
            val received = mutableListOf<Int>()
            val result =
                runCatching {
                    acknowledgedMessageFlow<Int>(null) {
                        callback = it
                        it.onMessage(first.queued())
                        return@acknowledgedMessageFlow {}
                    }.collect {
                        received.add(it)
                        callback.onMessage(second.queued())
                    }
                }
            assertSame(error, result.exceptionOrNull())
            assertEquals(listOf(1), received)
            assertEquals(1, first.rejections)
            assertEquals(1, second.rejections)
            assertEquals(0, second.checks)
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
