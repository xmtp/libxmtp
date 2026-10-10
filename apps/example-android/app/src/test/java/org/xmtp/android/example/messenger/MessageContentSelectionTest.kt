package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class MessageContentSelectionTest {
    private fun message(content: MessageContent) =
        Message(
            MessageData(
                id = "generated-message",
                clientKey = 0uL,
                conversationId = "generated-conversation",
                topic = "fixture-topic",
                senderInboxId = "sender",
                sentAt = Timestamp(10),
                insertedAt = Timestamp(11),
                expiresAt = null,
                kind = MessageKind.APPLICATION,
                deliveryStatus = DeliveryStatus.PUBLISHED,
                rawBytes = byteArrayOf(),
                contentType = ContentTypeId("xmtp.org", "text", 1u, 0u),
                fallback = null,
                encoded = null,
                content = content,
                replyCount = 0uL,
                reactions = emptyList(),
                inReplyTo = null,
            ),
        )

    @Test fun publicGeneratedMessageExposesTextThroughItsStandardWrapper() {
        val incoming = message(MessageContent.Text("seed 4/25"))
        assertTrue(incoming.content is SDKMessageContent.Standard)
        val selected = incoming.standardContent()
        assertTrue("The actual generated SDK text must count as a seed text", selected is MessageContent.Text)
        assertEquals("seed 4/25", (selected as MessageContent.Text).v1)
        assertEquals(
            "The production row mapper must use that same real wrapper",
            "seed 4/25",
            incoming.toRow("recipient").text,
        )
        assertFalse(
            "Other standard content must not count as a seeded text",
            message(MessageContent.Markdown("not a seeded text")).standardContent() is MessageContent.Text,
        )
    }
}
