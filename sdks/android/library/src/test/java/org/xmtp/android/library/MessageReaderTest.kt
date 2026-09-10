package org.xmtp.android.library

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

private const val MESSAGE_READER_TEST_TIMEOUT_MS = 10_000L

class MessageReaderTest {
    private class Delivery(
        val value: Int = 1,
        val current: Boolean = true,
        val decodeError: Throwable? = null,
        val acknowledgementError: Throwable? = null,
        val onCheck: () -> Unit = {},
    ) {
        var checks = 0
        var acknowledgements = 0
        var rejections = 0

        fun queued(): QueuedMessageDelivery<Int> =
            QueuedMessageDelivery(
                decode = {
                    decodeError?.let { throw it }
                    value
                },
                checkOwner = {
                    checks++
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
            val first = Delivery()
            val second = Delivery(2)
            val items = mutableListOf(first.queued(), second.queued())
            var ended = 0
            val reader =
                AcknowledgedMessageReader(
                    read = { items.removeAt(0) },
                    end = { ended++ },
                )
            assertEquals(1, reader.next())
            assertEquals(1, first.checks)
            assertEquals(0, first.acknowledgements)
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
            var ended = 0
            lateinit var item: Delivery
            val task =
                launch {
                    val context = coroutineContext
                    item = Delivery(onCheck = { context.cancel() })
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

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun selectionChangeDiscardsWithoutAcknowledgingAndReadsFreshSelection() =
        runBlocking {
            val stale = Delivery(current = false)
            val fresh = Delivery(2)
            val items = mutableListOf(stale.queued(), fresh.queued())
            val reader = AcknowledgedMessageReader(read = { items.removeAt(0) }, end = {})
            assertEquals(2, reader.next())
            assertEquals(1, stale.rejections)
            assertEquals(0, stale.acknowledgements)
            assertEquals(1, fresh.checks)
            assertEquals(0, fresh.acknowledgements)
            reader.close()
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun acknowledgementFailureRetainsTheTokenForExplicitRejection() =
        runBlocking {
            val error = IllegalStateException("acknowledgement failed")
            val first = Delivery(acknowledgementError = error)
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
            assertEquals(1, reader.next())
            assertSame(error, runCatching { reader.next() }.exceptionOrNull())
            assertEquals(1, reads)
            assertEquals(1, first.rejections)
            assertEquals(0, first.acknowledgements)
            assertEquals(1, ended)
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun decodeFailureClosesWithoutAcknowledgement() =
        runBlocking {
            val error = IllegalStateException("decode failed")
            val item = Delivery(decodeError = error)
            val reader = AcknowledgedMessageReader(read = { item.queued() }, end = {})
            assertSame(error, runCatching { reader.next() }.exceptionOrNull())
            assertEquals(0, item.checks)
            assertEquals(0, item.acknowledgements)
            assertEquals(1, item.rejections)
            assertNull(reader.next())
        }

    @Test(timeout = MESSAGE_READER_TEST_TIMEOUT_MS)
    fun closeDuringOwnershipCheckPreventsHandoff() =
        runBlocking {
            lateinit var reader: AcknowledgedMessageReader<Int>
            val item = Delivery(onCheck = { reader.close() })
            reader = AcknowledgedMessageReader(read = { item.queued() }, end = {})
            assertNull(reader.next())
            assertEquals(1, item.rejections)
            assertEquals(0, item.acknowledgements)
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
