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
