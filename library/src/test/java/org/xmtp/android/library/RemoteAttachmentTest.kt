package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Ignore
import org.junit.Test
import org.xmtp.android.library.codecs.Attachment
import org.xmtp.android.library.codecs.AttachmentCodec
import org.xmtp.android.library.codecs.ContentTypeAttachment
import org.xmtp.android.library.codecs.ContentTypeRemoteAttachment
import org.xmtp.android.library.codecs.RemoteAttachment
import org.xmtp.android.library.codecs.RemoteAttachmentCodec
import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.codecs.id
import org.xmtp.android.library.messages.walletAddress
import java.io.File
import java.net.URL

class RemoteAttachmentTest {

    @Test
    fun testEncryptedContentShouldBeDecryptable() {
        Client.register(codec = AttachmentCodec())
        val attachment = Attachment(
            filename = "test.txt",
            mimeType = "text/plain",
            data = "hello world".toByteStringUtf8(),
        )

        val encrypted = RemoteAttachment.encodeEncrypted(attachment, AttachmentCodec())

        val decrypted = RemoteAttachment.decryptEncoded(encrypted)
        Assert.assertEquals(ContentTypeAttachment.id, decrypted.type.id)

        val decoded = decrypted.decoded<Attachment>()
        Assert.assertEquals("test.txt", decoded?.filename)
        Assert.assertEquals("text/plain", decoded?.mimeType)
        Assert.assertEquals("hello world", decoded?.data?.toStringUtf8())
    }

    @Test
    @Ignore("Flaky")
    fun testCanUseRemoteAttachmentCodec() {
        val attachment = Attachment(
            filename = "test.txt",
            mimeType = "text/plain",
            data = "hello world".toByteStringUtf8(),
        )

        Client.register(codec = AttachmentCodec())
        Client.register(codec = RemoteAttachmentCodec())

        val encodedEncryptedContent = RemoteAttachment.encodeEncrypted(
            content = attachment,
            codec = AttachmentCodec(),
        )

        File("abcdefg").writeBytes(encodedEncryptedContent.payload.toByteArray())

        val remoteAttachment = RemoteAttachment.from(
            url = URL("https://abcdefg"),
            encryptedEncodedContent = encodedEncryptedContent,
        )

        remoteAttachment.contentLength = attachment.data.size()
        remoteAttachment.filename = attachment.filename

        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        }

        runBlocking {
            aliceConversation.send(
                content = remoteAttachment,
                options = SendOptions(contentType = ContentTypeRemoteAttachment),
            )
        }

        val messages = runBlocking { aliceConversation.messages() }
        Assert.assertEquals(messages.size, 1)

        if (messages.size == 1) {
            val loadedRemoteAttachment: RemoteAttachment = messages[0].content()!!
            loadedRemoteAttachment.fetcher = TestFetcher()
            runBlocking {
                val attachment2: Attachment =
                    loadedRemoteAttachment.load() ?: throw XMTPException("did not get attachment")
                Assert.assertEquals("test.txt", attachment2.filename)
                Assert.assertEquals("text/plain", attachment2.mimeType)
                Assert.assertEquals("hello world".toByteStringUtf8(), attachment2.data)
            }
        }
    }

    @Test
    fun testCannotUseNonHTTPSURL() {
        val attachment = Attachment(
            filename = "test.txt",
            mimeType = "text/plain",
            data = "hello world".toByteStringUtf8(),
        )

        Client.register(codec = AttachmentCodec())
        Client.register(codec = RemoteAttachmentCodec())

        val encodedEncryptedContent = RemoteAttachment.encodeEncrypted(
            content = attachment,
            codec = AttachmentCodec(),
        )

        File("abcdefg").writeBytes(encodedEncryptedContent.payload.toByteArray())

        Assert.assertThrows(XMTPException::class.java) {
            RemoteAttachment.from(
                url = URL("http://abcdefg"),
                encryptedEncodedContent = encodedEncryptedContent,
            )
        }
    }

    @Test
    @Ignore("Flaky")
    fun testEnsuresContentDigestMatches() {
        val attachment = Attachment(
            filename = "test.txt",
            mimeType = "text/plain",
            data = "hello world".toByteStringUtf8(),
        )

        Client.register(codec = AttachmentCodec())
        Client.register(codec = RemoteAttachmentCodec())

        val encodedEncryptedContent = RemoteAttachment.encodeEncrypted(
            content = attachment,
            codec = AttachmentCodec(),
        )

        File("abcdefg").writeBytes(encodedEncryptedContent.payload.toByteArray())

        val remoteAttachment = RemoteAttachment.from(
            url = URL("https://abcdefg"),
            encryptedEncodedContent = encodedEncryptedContent,
        )

        remoteAttachment.contentLength = attachment.data.size()
        remoteAttachment.filename = attachment.filename

        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        }

        runBlocking {
            aliceConversation.send(
                content = remoteAttachment,
                options = SendOptions(contentType = ContentTypeRemoteAttachment),
            )
        }

        val messages = runBlocking { aliceConversation.messages() }
        Assert.assertEquals(messages.size, 1)

        // Tamper with the payload
        File("abcdefg").writeBytes("sup".toByteArray())

        if (messages.size == 1) {
            val loadedRemoteAttachment: RemoteAttachment = messages[0].content()!!
            loadedRemoteAttachment.fetcher = TestFetcher()
            Assert.assertThrows(XMTPException::class.java) {
                runBlocking {
                    val attachment: Attachment? = loadedRemoteAttachment.load()
                }
            }
        }
    }
}
