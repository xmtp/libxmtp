package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

// The standard codecs in SDKCodecs.kt are hand-written Kotlin. Each one maps a
// public record to and from a StandardContent variant. Rust tests cover the bytes;
// these tests cover the Kotlin field mapping, the variant casts and the type IDs.
class StandardCodecWrapperTest {
    private val wrongVariant = TextCodec().encode("not this codec")

    // IDs are lowercase hex. Distinct values make a swapped or dropped field fail.
    private val messageId = "a1".repeat(32)
    private val inboxId = "b2".repeat(32)

    private fun <T : Any> checkRejectsOtherVariant(codec: ContentCodec<T>) {
        assertThrows(XmtpException.InvalidArgument::class.java) { codec.decode(wrongVariant) }
    }

    @Test
    fun readReceiptCodecRoundTrips() {
        val codec = ReadReceiptCodec()
        assertEquals(ContentTypeId("xmtp.org", "readReceipt", 1u, 0u), codec.type)
        assertEquals(Unit, codec.decode(codec.encode(Unit)))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun reactionV2CodecKeepsEachField() {
        val codec = ReactionV2Codec()
        val value =
            ReactionV2Content(
                messageId,
                inboxId,
                Reaction("smile", ReactionAction.REMOVED, ReactionSchema.SHORTCODE),
            )
        assertEquals(ContentTypeId("xmtp.org", "reaction", 2u, 0u), codec.type)
        assertEquals(value, codec.decode(codec.encode(value)))
        val withoutInbox = value.copy(referenceInboxId = null)
        assertEquals(withoutInbox, codec.decode(codec.encode(withoutInbox)))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun replyCodecKeepsEachField() {
        val codec = ReplyCodec()
        val inner = TextCodec().encode("reply body")
        val value = ReplyContent(messageId, inboxId, inner)
        assertEquals(ContentTypeId("xmtp.org", "reply", 1u, 0u), codec.type)
        val decoded = codec.decode(codec.encode(value))
        assertEquals(value.reference, decoded.reference)
        assertEquals(value.referenceInboxId, decoded.referenceInboxId)
        assertEquals(inner.type, decoded.content.type)
        assertEquals("reply body", TextCodec().decode(decoded.content))
        assertNull(codec.decode(codec.encode(value.copy(referenceInboxId = null))).referenceInboxId)
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun attachmentCodecKeepsEachField() {
        val codec = AttachmentCodec()
        val value = Attachment("test.txt", "text/plain", "hello world".toByteArray())
        assertEquals(ContentTypeId("xmtp.org", "attachment", 1u, 0u), codec.type)
        val decoded = codec.decode(codec.encode(value))
        assertEquals(value.filename, decoded.filename)
        assertEquals(value.mimeType, decoded.mimeType)
        assertArrayEquals(value.content, decoded.content)
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun transactionReferenceCodecKeepsEachField() {
        val codec = TransactionReferenceCodec()
        val value =
            TransactionReference(
                "eip155",
                "0x1",
                "0xabc123",
                TransactionMetadata("transfer", "ETH", 0.05, 18u, "0xAlice", "0xBob"),
            )
        assertEquals(ContentTypeId("xmtp.org", "transactionReference", 1u, 0u), codec.type)
        assertEquals(value, codec.decode(codec.encode(value)))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun leaveRequestCodecNormalizesEmptyNote() {
        val codec = LeaveRequestCodec()
        assertEquals(ContentTypeId("xmtp.org", "leave_request", 1u, 0u), codec.type)
        assertArrayEquals(
            "note".toByteArray(),
            codec.decode(codec.encode(LeaveRequest("note".toByteArray()))).authenticatedNote,
        )
        assertNull(codec.decode(codec.encode(LeaveRequest(byteArrayOf()))).authenticatedNote)
        assertNull(codec.decode(codec.encode(LeaveRequest(null))).authenticatedNote)
        checkRejectsOtherVariant(codec)
    }
}
