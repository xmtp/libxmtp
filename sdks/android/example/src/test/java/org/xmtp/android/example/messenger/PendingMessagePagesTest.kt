package org.xmtp.android.example.messenger
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

internal fun storedMessage(
    index: Int,
    status: DeliveryStatus,
    parent: ReplyParent? = null,
) = Message(
    MessageData(
        id = index.toString(),
        clientKey = 0uL,
        conversationId = "chat",
        topic = "topic",
        senderInboxId = "own",
        sentAt = Timestamp(index.toLong()),
        insertedAt = Timestamp(index.toLong()),
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
    @Test fun partialStatusPagesKeepReadableRowsAndDoNotAdvanceAcrossMissingRows() =
        runBlocking {
            for (rawSize in listOf(2, 60)) {
                val raw = (1..rawSize).map { storedMessage(it, DeliveryStatus.UNPUBLISHED) }
                val before = (rawSize + 1).toLong()
                val missing = rawSize.toString()

                fun selected(options: ListMessagesOptions) =
                    raw
                        .filter {
                            it.deliveryStatus == options.deliveryStatus &&
                                (options.sentBefore == null || it.sentAt.ns < options.sentBefore!!.ns) &&
                                (options.sentAfter == null || it.sentAt.ns > options.sentAfter!!.ns)
                        }.sortedByDescending { it.sentAt.ns }
                val page =
                    pendingMessagePage(
                        before,
                        read = { selected(it).take(it.limit!!.toInt()).filter { row -> row.id != missing } },
                        count = { selected(it).size.toULong() },
                    )
                assertTrue("A readable result remains available", page.rows.isNotEmpty())
                assertFalse("The raw selection is not fully readable", page.complete)
                assertNotNull("A partial result must be visible", page.notice)
                assertEquals("Do not skip a missing timestamp bucket", before, page.nextBeforeNs)
                assertTrue(page.rows.size <= 50)
            }
        }

    @Test fun combinedStatusPagesExposeBothBacklogsWithTheSameBoundedSelection() =
        runBlocking {
            val rows =
                (1..90).map { index ->
                    storedMessage(
                        index,
                        when {
                            index <= 40 -> DeliveryStatus.FAILED
                            index <= 80 -> DeliveryStatus.UNPUBLISHED
                            else -> DeliveryStatus.PUBLISHED
                        },
                    )
                }
            val seen = mutableListOf<ListMessagesOptions>()

            fun selected(options: ListMessagesOptions): List<Message> {
                seen.add(options)
                assertEquals(MessageKind.APPLICATION, options.kind)
                assertEquals(visibleContentTypes, options.contentTypes)
                assertEquals(MessageOrder.DESCENDING, options.direction)
                assertEquals(MessageSortBy.SENT_AT, options.sortBy)
                assertNotEquals(DeliveryStatus.PUBLISHED, options.deliveryStatus)
                return rows.filter {
                    it.deliveryStatus == options.deliveryStatus &&
                        (options.sentBefore == null || it.sentAt.ns < options.sentBefore!!.ns) &&
                        (options.sentAfter == null || it.sentAt.ns > options.sentAfter!!.ns)
                }
            }

            suspend fun page(before: Long?) =
                pendingMessagePage(
                    before,
                    read = { selected(it).sortedByDescending { row -> row.sentAt.ns }.take(it.limit!!.toInt()) },
                    count = { selected(it).size.toULong() },
                )
            val newest = page(null)
            assertEquals(50, newest.rows.size)
            assertFalse(newest.complete)
            val older = page(newest.nextBeforeNs)
            assertEquals(30, older.rows.size)
            assertTrue(older.complete)
            assertEquals(
                (1..80).map(Int::toString).toSet(),
                (newest.rows + older.rows).map { it.id }.toSet(),
            )
            assertTrue(seen.all { it.limit == null || it.limit!! <= 51u })
            assertEquals(
                setOf(DeliveryStatus.UNPUBLISHED, DeliveryStatus.FAILED),
                seen.map { it.deliveryStatus }.toSet(),
            )
        }
}
