package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

// The standard codecs in SDKCodecs.kt are hand-written Kotlin. Each one maps a
// public record to and from a StandardContent variant and overrides the
// fallback and shouldPush hooks. Rust tests cover the bytes and the catalogue;
// these tests cover the Kotlin field mapping, the variant casts, the type IDs
// and the hook overrides.
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

    @Test
    fun textCodecUsesTheTextVariant() {
        val codec = TextCodec()
        assertEquals(ContentTypeId("xmtp.org", "text", 1u, 0u), codec.type)
        assertEquals("plain", codec.decode(codec.encode("plain")))
        assertThrows(XmtpException.InvalidArgument::class.java) { codec.decode(MarkdownCodec().encode("**md**")) }
    }

    @Test
    fun remoteAttachmentCodecsKeepEachField() {
        val codec = RemoteAttachmentCodec()
        assertEquals(ContentTypeId("xmtp.org", "remoteStaticAttachment", 1u, 0u), codec.type)
        assertEquals(remoteAttachment, codec.decode(codec.encode(remoteAttachment)))
        checkRejectsOtherVariant(codec)

        val multi = MultiRemoteAttachmentCodec()
        val value = MultiRemoteAttachment(listOf(remoteAttachment, remoteAttachment.copy(filename = null)))
        assertEquals(ContentTypeId("xmtp.org", "multiRemoteStaticAttachment", 1u, 0u), multi.type)
        assertEquals(value, multi.decode(multi.encode(value)))
        checkRejectsOtherVariant(multi)
    }

    @Test
    fun deleteMessageCodecKeepsTheMessageId() {
        val codec = DeleteMessageCodec()
        assertEquals(ContentTypeId("xmtp.org", "deleteMessage", 1u, 0u), codec.type)
        assertEquals(DeleteMessageContent(messageId), codec.decode(codec.encode(DeleteMessageContent(messageId))))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun markdownCodecUsesTheMarkdownVariant() {
        val codec = MarkdownCodec()
        assertEquals(ContentTypeId("xmtp.org", "markdown", 1u, 0u), codec.type)
        assertEquals("**bold**", codec.decode(codec.encode("**bold**")))
        // A text envelope is another variant, so the markdown codec rejects it.
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun walletSendCallsCodecKeepsEachField() {
        val codec = WalletSendCallsCodec()
        assertEquals(ContentTypeId("xmtp.org", "walletSendCalls", 1u, 0u), codec.type)
        assertEquals(walletSendCalls, codec.decode(codec.encode(walletSendCalls)))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun actionsCodecKeepsEachField() {
        val codec = ActionsCodec()
        assertEquals(ContentTypeId("coinbase.com", "actions", 1u, 0u), codec.type)
        assertEquals(actions, codec.decode(codec.encode(actions)))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun intentCodecKeepsEachField() {
        val codec = IntentCodec()
        assertEquals(ContentTypeId("coinbase.com", "intent", 1u, 0u), codec.type)
        assertEquals(intent, codec.decode(codec.encode(intent)))
        assertEquals(intent.copy(metadataJson = null), codec.decode(codec.encode(intent.copy(metadataJson = null))))
        checkRejectsOtherVariant(codec)
    }

    @Test
    fun groupUpdatedCodecKeepsEachField() {
        val codec = GroupUpdatedCodec()
        assertEquals(ContentTypeId("xmtp.org", "group_updated", 1u, 0u), codec.type)
        assertEquals(groupUpdated, codec.decode(codec.encode(groupUpdated)))
        checkRejectsOtherVariant(codec)
    }

    // The fallback override must return the envelope's fallback, and the
    // shouldPush override must return the catalogue default. The expected
    // values are literals from xmtp_content_types, so a changed override fails.
    @Test
    fun standardCodecHooksMatchTheEnvelopeAndCatalogueDefault() {
        checkHooks(TextCodec(), "hi", push = true, fallback = null)
        checkHooks(MarkdownCodec(), "**hi**", push = true, fallback = null)
        checkHooks(ReadReceiptCodec(), Unit, push = false, fallback = null)
        checkHooks(
            ReactionV2Codec(),
            ReactionV2Content(messageId, inboxId, Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE)),
            push = false,
            fallback = "Reacted with \"smile\" to an earlier message",
        )
        checkHooks(
            AttachmentCodec(),
            Attachment("test.txt", "text/plain", byteArrayOf(1)),
            push = true,
            fallback = "Can't display test.txt. This app doesn't support attachments.",
        )
        checkHooks(RemoteAttachmentCodec(), remoteAttachment, push = true, fallback = null, hasFallback = true)
        checkHooks(
            MultiRemoteAttachmentCodec(),
            MultiRemoteAttachment(listOf(remoteAttachment)),
            push = true,
            fallback = "Can't display this content. This app doesn't support multiple remote attachments.",
        )
        checkHooks(
            TransactionReferenceCodec(),
            TransactionReference(null, "1", "0xabc", null),
            push = true,
            fallback = null,
            hasFallback = true,
        )
        checkHooks(WalletSendCallsCodec(), walletSendCalls, push = true, fallback = null, hasFallback = true)
        checkHooks(
            ActionsCodec(),
            actions,
            push = true,
            fallback = "Choose one\n\n[1] One\n\nReply with the number to select",
        )
        checkHooks(IntentCodec(), intent, push = true, fallback = "User selected action: one")
        checkHooks(
            ReplyCodec(),
            ReplyContent(messageId, inboxId, TextCodec().encode("reply body")),
            push = true,
            fallback = null,
            hasFallback = true,
        )
        checkHooks(GroupUpdatedCodec(), groupUpdated, push = false, fallback = null)
        checkHooks(DeleteMessageCodec(), DeleteMessageContent(messageId), push = false, fallback = null)
        checkHooks(
            LeaveRequestCodec(),
            LeaveRequest(null),
            push = false,
            fallback = "A member has requested leaving the group",
        )
    }

    // `fallback` is the literal expected text. Pass null with `hasFallback`
    // when the text depends on Rust formatting this test does not pin.
    private fun <T : Any> checkHooks(
        codec: ContentCodec<T>,
        value: T,
        push: Boolean,
        fallback: String?,
        hasFallback: Boolean = fallback != null,
    ) {
        val label = codec::class.simpleName
        assertEquals("$label shouldPush", push, codec.shouldPush(value))
        val envelope = codec.encode(value).fallback
        assertEquals("$label fallback", envelope, codec.fallback(value))
        if (fallback != null) assertEquals("$label fallback text", fallback, envelope)
        if (hasFallback) assertNotNull("$label fallback", envelope) else assertNull("$label fallback", envelope)
    }

    private val remoteAttachment =
        RemoteAttachment(
            "https://example.test/file",
            "digest",
            ByteArray(32) { 1 },
            ByteArray(32) { 2 },
            ByteArray(12) { 3 },
            "https",
            10u,
            "file",
        )

    private val walletSendCalls =
        WalletSendCalls(
            "1.0",
            "0x1",
            "0xsender",
            listOf(
                WalletCall(
                    "0xto",
                    "0xdata",
                    "0x10",
                    "0x5208",
                    WalletCallMetadata("Send funds", "transfer", mapOf("note" to "rent")),
                ),
                WalletCall(null, null, null, null, null),
            ),
            mapOf("paymasterService" to "https://paymaster.example.test"),
        )

    private val actions =
        Actions(
            "actions-1",
            "Choose one",
            listOf(Action("one", "One", "https://example.test/one.png", ActionStyle.PRIMARY, null)),
            null,
        )

    private val intent = Intent("actions-1", "one", """{"source":"test"}""")

    private val groupUpdated =
        GroupUpdated(
            inboxId,
            listOf("c3".repeat(32)),
            listOf("d4".repeat(32)),
            listOf("e5".repeat(32)),
            listOf(MetadataFieldChange("group_name", "old", "new"), MetadataFieldChange("description", null, "set")),
            listOf("f6".repeat(32)),
            listOf("a7".repeat(32)),
            listOf("b8".repeat(32)),
            listOf("c9".repeat(32)),
        )
}
