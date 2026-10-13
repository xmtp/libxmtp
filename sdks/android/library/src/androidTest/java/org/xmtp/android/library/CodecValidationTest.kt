package org.xmtp.android.library

import android.util.Base64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.xmtp_sdk.*

class CodecValidationTest : BaseInstrumentedTest() {
    // verifies: CTYPE-014, CTYPE-026
    @Test
    fun remoteFieldsAndLoopbackUrls() {
        val codec = RemoteAttachmentCodec()
        for (url in listOf(
            "https://example.test/file",
            "http://localhost/file",
            "http://127.0.0.1/file",
            "http://[::1]/file",
        )) {
            val value =
                RemoteAttachment(
                    url,
                    "ab".repeat(32),
                    ByteArray(32) {
                        1
                    },
                    ByteArray(
                        32,
                    ) { 2 },
                    ByteArray(12) { 3 },
                    if (url.startsWith("https")) "https://" else "http://",
                    null,
                    null,
                )
            val encoded = codec.encode(value)
            assertEquals(url, codec.decode(encoded).url)
            for (field in listOf("contentDigest", "secret", "salt", "nonce", "scheme")) {
                val missing = encoded.copy(parameters = encoded.parameters - field)
                assertThrows(XmtpException.CodecDecodeFailed::class.java) { codec.decode(missing) }
            }
        }
    }

    // verifies: CTYPE-030
    @Test
    fun actionsExpiryRangeOnBothFields() {
        val codec = ActionsCodec()
        for ((timestamp, valid) in listOf(
            "1677-09-21T00:12:43.145224192Z" to true,
            "2262-04-11T23:47:16.854775807Z" to true,
            "1677-09-21T00:12:43.145224191Z" to false,
            "2262-04-11T23:47:16.854775808Z" to false,
        )) {
            for (nested in listOf(false, true)) {
                val json =
                    if (nested) {
                        """
                        {"id":"choices","description":"Choose",
                         "actions":[{"id":"one","label":"One","expiresAt":"$timestamp"}]}
                        """.trimIndent()
                    } else {
                        """
                        {"id":"choices","description":"Choose","expiresAt":"$timestamp",
                         "actions":[{"id":"one","label":"One"}]}
                        """.trimIndent()
                    }
                val encoded = EncodedContent(codec.type, emptyMap(), null, json.toByteArray())
                if (valid) {
                    val decoded = codec.decode(encoded)
                    assertEquals(
                        if (timestamp.startsWith("1677")) Long.MIN_VALUE else Long.MAX_VALUE,
                        (if (nested) decoded.actions[0].expiresAt else decoded.expiresAt)?.ns,
                    )
                } else {
                    assertThrows(XmtpException.CodecDecodeFailed::class.java) { codec.decode(encoded) }
                }
            }
        }
        val value =
            Actions("choices", "Choose", listOf(Action("one", "One", null, null, null)), Timestamp(Long.MIN_VALUE))
        val error = assertThrows(XmtpException.InvalidInput::class.java) { codec.encode(value) }
        assertEquals("InvalidInput", error.v1.code)
        assertEquals(ErrorCategory.INPUT, error.v1.category)
        org.junit.Assert.assertFalse(error.v1.retryable)
    }

    private fun wire(
        content: ByteArray,
        algorithm: Byte,
    ): ByteArray = encodeEncodedContent(TextCodec().encode("").copy(content = content)) + byteArrayOf(0x28, algorithm)

    // verifies: CTYPE-024, CTYPE-025, CTYPE-026
    @Test
    fun standaloneAndNestedDecodingHaveAnInclusiveBound() {
        // Each independent gzip member expands to 1 MiB of A bytes.
        val member =
            Base64.decode(
                "H4sIAAAAAAAAE+3BMQEAAADCoGzrX8oQvkABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHwGyb72gQAAEAA=",
                Base64.DEFAULT,
            )
        val extra = Base64.decode("H4sIAAAAAAAAE3MEAIue2dMBAAAA", Base64.DEFAULT)
        val exact = ByteArray(member.size * 16) { member[it % member.size] }
        assertEquals(16_777_216, decodeEncodedContent(wire(exact, 1)).content.size)
        val overflow = wire(exact + extra, 1)
        assertThrows(XmtpException.CodecDecodeFailed::class.java) { decodeEncodedContent(overflow) }
        val reply =
            ReplyCodec()
                .encode(
                    ReplyContent("a".repeat(64), content = TextCodec().encode("body")),
                ).copy(content = overflow)
        assertThrows(XmtpException.CodecDecodeFailed::class.java) { ReplyCodec().decode(reply) }
        assertThrows(XmtpException.CodecDecodeFailed::class.java) { decodeEncodedContent(wire(byteArrayOf(1), 99)) }
    }

    // verifies: CTYPE-001, CTYPE-009, CTYPE-023, CTYPE-024
    @Test
    fun registryKeysAndSharedCompressionFrames() =
        kotlinx.coroutines.runBlocking {
            val noteType = ContentTypeId("example.org", "note", 1u, 0u)
            val codec =
                object : ContentCodec<String> {
                    override val type = noteType

                    override fun encode(value: String) = EncodedContent(type, content = value.toByteArray())

                    override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()
                }
            val client = createClient(generateLocalSigner(), codecs = listOf(codec))
            val group = client.conversations.createGroup(emptyList())
            for ((changed, matched) in listOf(
                noteType.copy(versionMinor = 9u) to true,
                noteType.copy(authorityId = "other.org") to false,
                noteType.copy(typeId = "other") to false,
                noteType.copy(versionMajor = 2u) to false,
            )) {
                val encoded = codec.encode("value").copy(type = changed)
                val id = group.send(encoded)
                val message = checkNotNull(client.conversations.getMessageById(id))
                if (matched) {
                    assertEquals("value", (message.content as SDKMessageContent.Custom).value)
                } else {
                    assertEquals("CodecNotFound", (message.content as SDKMessageContent.Unknown).error.code)
                }
            }
            for ((compression, magic) in listOf(
                Compression.DEFLATE to byteArrayOf(0x78, 0x9c.toByte()),
                Compression.GZIP to byteArrayOf(0x1f, 0x8b.toByte()),
            )) {
                val text = "compressed".repeat(100)
                val id = group.sendText(text, SendOptions(compression = compression))
                val message = checkNotNull(client.conversations.getMessageById(id))
                assertEquals(MessageContent.Text(text), (message.content as SDKMessageContent.Standard).value)
                org.junit.Assert.assertTrue(
                    (0 until message.rawBytes.size - 1).any {
                        message.rawBytes[it] == magic[0] &&
                            message.rawBytes[it + 1] == magic[1]
                    },
                )
            }
        }

    // verifies: CTYPE-009
    @Test
    fun standaloneCodecFailuresHaveDistinctTypedOutcomes() {
        val text = TextCodec().encode("body")
        val attempts =
            listOf<Pair<String, () -> Any>>(
                "InvalidInput" to { ActionsCodec().encode(Actions("choices", "Choose", emptyList(), null)) },
                "CodecDecodeFailed" to { decodeStandard(text.copy(content = byteArrayOf(0xff.toByte()))) },
                "CodecNotFound" to { decodeStandard(text.copy(type = text.type.copy(authorityId = "other.example"))) },
                "MalformedEnvelope" to { decodeEncodedContent(byteArrayOf(0xff.toByte())) },
            )
        for ((code, operation) in attempts) {
            val error = runCatching(operation).exceptionOrNull()
            val details =
                when (error) {
                    is XmtpException.InvalidInput -> error.v1
                    is XmtpException.CodecDecodeFailed -> error.v1
                    is XmtpException.CodecNotFound -> error.v1
                    is XmtpException.MalformedEnvelope -> error.v1
                    else -> throw AssertionError("Expected typed codec failure", error)
                }
            assertEquals(code, details.code)
            assertEquals(ErrorCategory.INPUT, details.category)
            org.junit.Assert.assertFalse(details.retryable)
            org.junit.Assert.assertFalse(details.message.isEmpty())
        }
    }
}
