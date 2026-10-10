package org.xmtp.android.example.messenger
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

internal fun storedMessage(
    index: Int,
    status: DeliveryStatus,
    parent: ReplyParent? = null,
    timeNs: Long = index.toLong(),
) = Message(
    MessageData(
        id = index.toString(),
        clientKey = 0uL,
        conversationId = "chat",
        topic = "topic",
        senderInboxId = "own",
        sentAt = Timestamp(timeNs),
        insertedAt = Timestamp(timeNs),
        expiresAt = null,
        kind = MessageKind.APPLICATION,
        deliveryStatus = status,
        rawBytes = byteArrayOf(),
        contentType = ContentTypeId("xmtp.org", "text", 1u, 0u),
        fallback = null,
        encoded = null,
        content = MessageContent.Text("Message $index"),
        replyCount = 0uL,
        reactions = emptyList(),
        inReplyTo = parent,
    ),
)

class PendingMessagePagesTest {
    private fun position(index: Int) = MessageRecoveryPosition(Timestamp(777), "opaque/${1000 - index}/=?")

    private fun assertSelection(options: ListMessagesOptions) {
        assertEquals(MessageKind.APPLICATION, options.kind)
        assertEquals(visibleContentTypes, options.contentTypes)
        assertEquals(MessageOrder.DESCENDING, options.direction)
        assertEquals(MessageSortBy.SENT_AT, options.sortBy)
        assertNull("One SDK query selects both pending statuses", options.deliveryStatus)
        assertEquals(50u, options.limit)
    }

    @Test fun tiedPendingRowsKeepSdkOrderAndReachEveryIdThroughBoundedPages() =
        runBlocking {
            val rows =
                (501 downTo 1).map {
                    storedMessage(
                        it,
                        if (it % 2 == 0) DeliveryStatus.FAILED else DeliveryStatus.UNPUBLISHED,
                        timeNs = 777,
                    )
                }
            val offsets = rows.indices.associate { position(it) to it }
            var calls = 0
            val read: suspend (ListMessagesOptions, MessageRecoveryPosition?, MessageRecoveryPosition?) ->
            MessageRecoveryPage = { options, before, after ->
                assertSelection(options)
                assertNull(after)
                calls += 1
                val start = before?.let { checkNotNull(offsets[it]) + 1 } ?: 0
                val selected = rows.drop(start).take(50)
                MessageRecoveryPage(
                    selected,
                    position(start),
                    position(start + selected.size - 1),
                    start + selected.size < rows.size,
                    0u,
                )
            }
            val visible = mutableListOf<String>()
            var before: MessageRecoveryPosition? = null
            do {
                val page = pendingMessagePage(before, read)
                assertTrue("The recovery overlay stays bounded", page.rows.size <= 50)
                visible += page.rows.map { it.id }
                before = page.last
            } while (page.hasOlder)
            assertEquals("All SDK ordered IDs remain reachable", rows.map { it.id }, visible)
            assertEquals(501, visible.toSet().size)
            assertEquals(11, calls)
        }

    @Test fun emptyRawPagesKeepContinuationAtTheFourReadLimit() =
        runBlocking {
            var calls = 0
            val bounds = mutableListOf<MessageRecoveryPosition?>()
            val read: suspend (ListMessagesOptions, MessageRecoveryPosition?, MessageRecoveryPosition?) ->
            MessageRecoveryPage = { options, before, after ->
                assertSelection(options)
                assertNull(after)
                bounds += before
                calls += 1
                if (calls <= 4) {
                    MessageRecoveryPage(
                        emptyList(),
                        position((calls - 1) * 50),
                        position(calls * 50 - 1),
                        true,
                        50u,
                    )
                } else {
                    MessageRecoveryPage(
                        listOf(storedMessage(1, DeliveryStatus.FAILED)),
                        position(200),
                        position(200),
                        false,
                        0u,
                    )
                }
            }
            val empty = pendingMessagePage(null, read)
            assertEquals("The first operation uses four recovery calls", 4, calls)
            assertTrue(empty.rows.isEmpty())
            assertTrue("Raw continuation remains available", empty.hasOlder)
            assertEquals(position(0), empty.first)
            assertEquals(position(199), empty.last)
            assertNotNull(empty.notice)
            val readable = pendingMessagePage(empty.last, read)
            assertEquals(listOf("1"), readable.rows.map { it.id })
            assertFalse(readable.hasOlder)
            assertEquals(listOf(null, position(49), position(99), position(149), position(199)), bounds)
        }

    @Test fun partialRawPageKeepsReadableRowsNoticeAndContinuation() =
        runBlocking {
            val retained = storedMessage(7, DeliveryStatus.UNPUBLISHED)
            var calls = 0
            val page =
                pendingMessagePage(null) { options, before, after ->
                    assertSelection(options)
                    assertNull(before)
                    assertNull(after)
                    calls += 1
                    MessageRecoveryPage(listOf(retained), position(0), position(49), true, 49u)
                }
            assertEquals(1, calls)
            assertEquals(listOf("7"), page.rows.map { it.id })
            assertNotNull("Conversion loss stays visible", page.notice)
            assertTrue("A notice does not remove raw continuation", page.hasOlder)
            assertEquals(position(49), page.last)
        }

    @Test fun heldPageRefreshUsesTheSameOpaqueUpperBound() =
        runBlocking {
            val upper = position(49)
            val requested = mutableListOf<MessageRecoveryPosition?>()
            var removed = false
            val read: suspend (ListMessagesOptions, MessageRecoveryPosition?, MessageRecoveryPosition?) ->
            MessageRecoveryPage = { options, before, after ->
                assertSelection(options)
                assertEquals(upper, before)
                assertNull(after)
                requested += before
                val rows =
                    (100 downTo 51).filter { !removed || it != 80 }.map {
                        storedMessage(it, DeliveryStatus.UNPUBLISHED, timeNs = 777)
                    }
                MessageRecoveryPage(rows, position(50), position(99), true, 0u)
            }
            val initial = pendingMessagePage(upper, read)
            removed = true
            val refreshed = pendingMessagePage(upper, read)
            assertEquals(listOf(upper, upper), requested)
            assertEquals(50, initial.rows.size)
            assertEquals(49, refreshed.rows.size)
            assertFalse("Refresh removes a row that left the pending selection", refreshed.rows.any { it.id == "80" })
            assertEquals(position(99), refreshed.last)
            assertTrue(refreshed.hasOlder)
        }
}
