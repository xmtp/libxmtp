package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.xmtp_sdk.*
import java.net.URL

class RemoteAttachmentTest {
    @get:Rule val files = TemporaryFolder()
    private val attachment = Attachment("test.txt", "text/plain", "hello world".toByteArray())

    private suspend fun encrypt(): EncryptedEncodedContent =
        encryptEncodedContent(encodeEncodedContent(AttachmentCodec().encode(attachment)))

    private fun assertAttachment(actual: Attachment) {
        assertEquals(attachment.filename, actual.filename)
        assertEquals(attachment.mimeType, actual.mimeType)
        assertArrayEquals(attachment.content, actual.content)
    }

    private suspend fun load(
        remote: RemoteAttachment,
        fetcher: TestFetcher,
    ): Attachment {
        val ciphertext = fetcher.fetch(URL(remote.url))
        val keys =
            EncryptionKeys(
                remote.secret,
                remote.salt,
                remote.nonce,
                remote.contentDigest,
                checkNotNull(remote.contentLength).toULong(),
            )
        return AttachmentCodec().decode(decodeEncodedContent(decryptBytes(ciphertext, keys)))
    }

    @Test fun testEncryptedContentShouldBeDecryptable() =
        runBlocking {
            val encrypted = encrypt()
            val decoded = decodeEncodedContent(decryptEncodedContent(encrypted))
            assertEquals(AttachmentCodec().type, decoded.type)
            assertAttachment(AttachmentCodec().decode(decoded))
        }

    @Test fun testCanUseRemoteAttachmentCodec() =
        runBlocking {
            val encrypted = encrypt()
            val file = files.newFile("ciphertext").also { it.writeBytes(encrypted.ciphertext) }
            val remote = remoteAttachmentFromEncrypted("https://example.com/attachment", encrypted, attachment.filename)
            assertEquals(encrypted.ciphertext.size.toUInt(), remote.contentLength)
            fixtures().use { fixtures ->
                val dm = fixtures.aliceClient.conversations().createDm(fixtures.bobClient.inboxId())
                val id = dm.sendRemoteAttachment(remote)
                val applications = dm.messages(ListMessagesOptions(kind = MessageKind.APPLICATION))
                assertEquals(1, applications.size)
                assertEquals(id, applications.single().id)
                val receivedContent = (applications.single().content as SDKMessageContent.Standard).value
                val received = (receivedContent as MessageContent.RemoteAttachment).v1
                assertEquals(remote, received)
                assertEquals(remote, RemoteAttachmentCodec().decode(RemoteAttachmentCodec().encode(remote)))
                assertAttachment(load(received, TestFetcher(file)))
            }
        }

    @Test fun testCannotUseNonHTTPSURL() =
        runBlocking {
            val encrypted = encrypt()
            val error =
                assertThrows(XmtpException.InvalidArgument::class.java) {
                    remoteAttachmentFromEncrypted("http://abcdefg", encrypted, attachment.filename)
                }
            assertEquals("InvalidArgument", error.v1.code)
            assertEquals(ErrorCategory.INPUT, error.v1.category)
            assertFalse(error.v1.retryable)
        }

    @Test fun testEnsuresContentDigestMatches() =
        runBlocking {
            val encrypted = encrypt()
            val file = files.newFile("ciphertext").also { it.writeBytes(encrypted.ciphertext) }
            val remote = remoteAttachmentFromEncrypted("https://example.com/attachment", encrypted, attachment.filename)
            fixtures().use { fixtures ->
                val dm = fixtures.aliceClient.conversations().createDm(fixtures.bobClient.inboxId())
                val id = dm.sendRemoteAttachment(remote)
                val message = dm.messages().single { it.id == id }
                val received =
                    ((message.content as SDKMessageContent.Standard).value as MessageContent.RemoteAttachment)
                        .v1
                file.writeBytes(encrypted.ciphertext.copyOf().also { it[0] = (it[0].toInt() xor 1).toByte() })
                try {
                    load(received, TestFetcher(file))
                    fail("Changed attachment bytes must fail verification")
                } catch (error: XmtpException.Unknown) {
                    assertFalse(error.v1.retryable)
                    assertTrue(error.v1.message.contains("content digest mismatch"))
                }
            }
        }
}
