package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.xmtp_sdk.*

// The Message getters are generated in MessageFields.kt and forward to MessageData.
// Each field has a distinct value, so a getter that reads the wrong field fails.
class MessageGettersTest {
    @Test
    fun gettersForwardTheirOwnField() {
        val reaction =
            ReactionMessage(
                "0a".repeat(32),
                "0b".repeat(32),
                Timestamp(5),
                DeliveryStatus.UNPUBLISHED,
                Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE),
            )
        val data =
            deliveryTestMessage(fallback = "fallback text").data.copy(
                expiresAt = Timestamp(3),
                replyCount = 4uL,
                reactions = listOf(reaction),
            )
        val message = Message(data)
        assertEquals(Timestamp(1), message.sentAt)
        assertEquals(Timestamp(2), message.insertedAt)
        assertEquals(Timestamp(3), message.expiresAt)
        assertEquals(listOf(reaction), message.reactions)
        assertEquals(4uL, message.replyCount)
        assertEquals("01".repeat(32), message.id)
        assertEquals("02".repeat(16), message.conversationId)
        assertEquals("03".repeat(32), message.senderInboxId)
        assertEquals("test-topic", message.topic)
        assertEquals("cursor-${"01".repeat(32)}", message.deliveryCursor)
        assertEquals("fallback text", message.fallback)
        assertEquals(MessageKind.APPLICATION, message.kind)
        assertEquals(DeliveryStatus.PUBLISHED, message.deliveryStatus)
    }
}
