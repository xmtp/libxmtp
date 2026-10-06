package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.RecordingMessageReader
import org.xmtp.android.library.RecordingReaderClient
import org.xmtp.android.library.deliveryTestMessage
import org.xmtp.android.library.testSDKClient

// The receive codec registry (SDKClient.kt) and the Message content projection
// (SDKTypes.kt) are hand-written Kotlin. Rust keeps the bytes and the failure
// details: xmtp_sdk/src/tests/retained_content.rs::failed_content_keeps_bytes_details_and_stream_progress.
// MessageReaderTest checks that a failed custom decode keeps the stream open.
class CustomCodecTest {
    private val noteType = ContentTypeId("example.org", "note", 1u, 0u)

    private class NoteCodec(
        override val type: ContentTypeId,
        private val failure: Throwable? = null,
    ) : ContentCodec<String> {
        override fun encode(value: String) = EncodedContent(type, emptyMap(), null, value.toByteArray())

        override fun decode(encoded: EncodedContent): String {
            failure?.let { throw it }
            return encoded.content.decodeToString()
        }
    }

    /** A failure whose text cannot be read. */
    private class HostileFailure : RuntimeException("hostile codec failure") {
        override fun toString(): String = throw IllegalStateException("hostile diagnostic conversion")
    }

    private fun <T> withClient(
        codecs: List<ContentCodec<*>>,
        block: (ULong) -> T,
    ): T {
        val raw = RecordingReaderClient { RecordingMessageReader { null } }
        val client = testSDKClient(raw, codecs)
        ClientRegistry.register(client)
        try {
            return block(raw.key)
        } finally {
            ClientRegistry.remove(client)
        }
    }

    private fun custom(
        clientKey: ULong,
        encoded: EncodedContent,
        raw: ByteArray = byteArrayOf(7, 8),
    ) = deliveryTestMessage(MessageContent.Custom(encoded, raw), raw, encoded = encoded, clientKey = clientKey)

    private fun reply(
        clientKey: ULong,
        body: MessageBody,
        raw: ByteArray = byteArrayOf(5, 6),
    ) = deliveryTestMessage(MessageContent.Reply("0f".repeat(32), body), raw, clientKey = clientKey)

    // verifies: CTYPE-027
    @Test
    fun codecStaysWithItsClient() {
        val encoded = NoteCodec(noteType).encode("codec value")
        withClient(listOf(NoteCodec(noteType))) { key ->
            val decoded = custom(key, encoded).content as SDKMessageContent.Custom
            assertEquals("codec value", decoded.value)
            assertNull(decoded.error)
            val body = reply(key, MessageBody.Custom(encoded, byteArrayOf(1))).replyContent
            assertEquals(
                "A reply body did not run the custom codec",
                "codec value",
                (body as SDKReplyContent.Custom).value,
            )
        }
        withClient(emptyList()) { key ->
            val unknown = custom(key, encoded).content as SDKMessageContent.Unknown
            assertEquals("CodecNotFound", unknown.error.code)
            assertEquals(ErrorCategory.INPUT, unknown.error.category)
            assertEquals(encoded, unknown.encoded)
        }
        // The codec key keeps the authority and type ID apart.
        withClient(listOf(NoteCodec(ContentTypeId("example.org", "a/b", 1u, 0u)))) { key ->
            val colliding =
                EncodedContent(ContentTypeId("example.org/a", "b", 1u, 0u), emptyMap(), null, byteArrayOf(1))
            assertTrue(
                "A codec key collision selected the wrong codec",
                custom(key, colliding).content is SDKMessageContent.Unknown,
            )
        }
    }

    // verifies: CTYPE-008, CTYPE-009, CTYPE-029, PROC-045
    @Test
    fun decodeFailureKeepsBytesAndDetails() {
        val encoded = NoteCodec(noteType).encode("bad value")
        val raw = byteArrayOf(7, 8)
        withClient(listOf(NoteCodec(noteType, IllegalStateException("codec decode failed")))) { key ->
            val failed = custom(key, encoded, raw).content as SDKMessageContent.Custom
            assertNull(failed.value)
            assertArrayEquals(raw, failed.rawBytes)
            assertEquals(encoded, failed.encoded)
            val error = checkNotNull(failed.error)
            assertEquals("CodecDecodeFailed", error.code)
            assertEquals(ErrorCategory.CALLBACK, error.category)
            assertFalse(error.retryable)
            assertTrue(error.message.contains("codec decode failed"))

            // A reply whose own body fails to decode is Unknown as a whole.
            val replyRaw = byteArrayOf(5, 6)
            val nested = reply(key, MessageBody.Custom(encoded, raw), replyRaw)
            val outer = nested.content as SDKMessageContent.Unknown
            assertArrayEquals(replyRaw, outer.rawBytes)
            assertEquals("CodecDecodeFailed", outer.error.code)
            assertEquals(ErrorCategory.CALLBACK, outer.error.category)

            // A parent that fails to decode leaves a valid reply unchanged.
            val parent =
                ReplyParent(
                    "0e".repeat(32),
                    "03".repeat(32),
                    Timestamp(1),
                    MessageKind.APPLICATION,
                    DeliveryStatus.PUBLISHED,
                    raw,
                    noteType,
                    null,
                    encoded,
                    MessageBody.Custom(encoded, raw),
                )
            val child =
                Message(
                    deliveryTestMessage(clientKey = key).data.copy(
                        content = MessageContent.Text("valid reply"),
                        inReplyTo = parent,
                    ),
                )
            assertEquals(MessageContent.Text("valid reply"), (child.content as SDKMessageContent.Standard).value)
            val parentContent = child.inReplyToContent as SDKReplyContent.Custom
            assertArrayEquals(raw, parentContent.rawBytes)
            assertEquals("CodecDecodeFailed", parentContent.error?.code)
        }
        withClient(listOf(NoteCodec(noteType, HostileFailure()))) { key ->
            val failed = custom(key, encoded).content as SDKMessageContent.Custom
            assertEquals("custom content codec failed", failed.error?.message)
        }
    }

    @Test
    fun contentOfAnEndedClientIsClientClosed() {
        val encoded = NoteCodec(noteType).encode("value")
        val key = withClient(listOf(NoteCodec(noteType))) { it }
        val closed = custom(key, encoded).content as SDKMessageContent.Custom
        assertNull(closed.value)
        assertEquals("ClientClosed", closed.error?.code)
    }
}
