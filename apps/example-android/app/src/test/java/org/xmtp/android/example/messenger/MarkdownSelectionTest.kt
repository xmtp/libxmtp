package org.xmtp.android.example.messenger
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class MarkdownSelectionTest {
    @Test fun markdownParentKeepsTextWithoutAFallback() {
        val parent =
            ReplyParent(
                "parent",
                "peer",
                Timestamp(1),
                MessageKind.APPLICATION,
                DeliveryStatus.PUBLISHED,
                byteArrayOf(),
                ContentTypeId("xmtp.org", "markdown", 1u, 0u),
                null,
                null,
                MessageBody.Markdown("**First line**\nSecond line"),
            )
        val row = storedMessage(1, DeliveryStatus.PUBLISHED, parent).toRow("own")
        assertEquals("**First line**\nSecond line", row.reply)
    }

    @Test fun markdownUsesTheCurrentCodecTypeInPublishedAndUnreadSelections() {
        val type = ContentTypeId("xmtp.org", "markdown", 1u, 0u)
        assertEquals(ContentTypeId("xmtp.org", "markdown", 1u, 0u), type)
        assertTrue(publishedSelection().contentTypes!!.contains(type))
        assertTrue(incomingSelection("own").contentTypes!!.contains(type))
        assertEquals(DeliveryStatus.PUBLISHED, incomingSelection("own").deliveryStatus)
        assertEquals(MessageKind.APPLICATION, publishedSelection().kind)
    }
}
