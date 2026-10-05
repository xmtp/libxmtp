package uniffi.xmtp_sdk

import org.junit.Assert.*
import org.junit.Test

/** Check the generated JNI route without a client or a host transfer helper. */
class AndroidRemoteProjectionTest {
    @Test fun missingRequiredParametersFailThroughNativeDecoder() {
        val original =
            RemoteAttachment(
                "https://example.org/file?signature=value",
                "digest",
                ByteArray(32) { 1 },
                ByteArray(32) { 2 },
                ByteArray(12) { 3 },
                "https://",
                12u,
                "file",
            )
        val encoded = encodeStandard(StandardContent.RemoteAttachment(original))
        assertEquals(original, (decodeStandard(encoded) as StandardContent.RemoteAttachment).v1)
        for (field in listOf("contentDigest", "salt", "nonce", "secret", "scheme")) {
            val malformed = encoded.copy(parameters = encoded.parameters - field)
            val error =
                assertThrows("Missing $field must fail", XmtpException.CodecDecodeFailed::class.java) {
                    decodeStandard(malformed)
                }
            assertTrue(error.v1.message.contains("missing $field parameter"))
            assertFalse(error.v1.retryable)
            assertEquals(original, (decodeStandard(encoded) as StandardContent.RemoteAttachment).v1)
        }
        val optional = encoded.copy(parameters = encoded.parameters - setOf("contentLength", "filename"))
        val decoded = (decodeStandard(optional) as StandardContent.RemoteAttachment).v1
        assertNull(decoded.contentLength)
        assertNull(decoded.filename)
        assertEquals(original.url, decoded.url)
        assertArrayEquals(original.secret, decoded.secret)
        assertArrayEquals(original.salt, decoded.salt)
        assertArrayEquals(original.nonce, decoded.nonce)
    }

    @Test fun encryptedProjectionKeepsCiphertextFieldsAndNestedCodecRecords() {
        fun encrypted() =
            EncryptedEncodedContent(
                "ciphertext".toByteArray(),
                EncryptionKeys(ByteArray(32) { 1 }, ByteArray(32) { 2 }, ByteArray(12) { 3 }, "wrong digest", 999uL),
            )
        val records =
            listOf(
                "https://example.org/file?signature=value",
                "http://localhost/file",
                "http://127.0.0.1/file",
                "http://[::1]/file",
            ).map { url ->
                val record = remoteAttachmentFromEncrypted(url, encrypted(), "file")
                assertEquals(url, record.url)
                assertEquals("305531dcc50ebca31cf1d5b31e9fc76ed51f66b3b6dd5a030c6539ae6532f979", record.contentDigest)
                assertEquals(10u, record.contentLength)
                assertArrayEquals(ByteArray(32) { 1 }, record.secret)
                assertArrayEquals(ByteArray(32) { 2 }, record.salt)
                assertArrayEquals(ByteArray(12) { 3 }, record.nonce)
                assertEquals(if (url.startsWith("https:")) "https://" else "http://", record.scheme)
                assertEquals("file", record.filename)
                record
            }
        val optional = remoteAttachmentFromEncrypted("https://example.org/optional", encrypted(), null)
        assertNull(optional.filename)
        val codec = MultiRemoteAttachmentCodec()
        val nested = MultiRemoteAttachment(records + optional)
        assertEquals(nested, codec.decode(codec.encode(nested)))
        for (url in listOf("not a url", "ftp://example.org/file", "http://example.org/file")) {
            assertTrue(
                runCatching {
                    remoteAttachmentFromEncrypted(
                        url,
                        encrypted(),
                        null,
                    )
                }.exceptionOrNull() is XmtpException.InvalidArgument,
            )
        }
    }
}
