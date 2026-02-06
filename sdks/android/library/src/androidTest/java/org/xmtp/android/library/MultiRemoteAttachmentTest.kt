package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.Attachment
import org.xmtp.android.library.codecs.AttachmentCodec
import org.xmtp.android.library.codecs.ContentTypeMultiRemoteAttachment
import org.xmtp.android.library.codecs.ContentTypeText
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.EncryptedEncodedContent
import org.xmtp.android.library.codecs.MultiRemoteAttachment
import org.xmtp.android.library.codecs.MultiRemoteAttachmentCodec
import org.xmtp.android.library.codecs.RemoteAttachment
import org.xmtp.android.library.codecs.RemoteAttachmentCodec
import org.xmtp.android.library.codecs.RemoteAttachmentInfo
import org.xmtp.android.library.codecs.id
import uniffi.xmtpv3.FfiMultiRemoteAttachment
import java.net.URL
import kotlin.random.Random

@RunWith(AndroidJUnit4::class)
class MultiRemoteAttachmentTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client

    @org.junit.Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    private val encryptedPayloadUrls = HashMap<String, ByteArray>()

    private fun testUploadEncryptedPayload(encryptedPayload: ByteArray): String {
        val randomUrl: String = "https://" + Random(encryptedPayload.hashCode()).nextInt(0, 1000000)
        encryptedPayloadUrls.put(randomUrl, encryptedPayload)
        return randomUrl
    }

    @Test
    fun testCanUseMultiRemoteAttachmentCodec() {
        Client.register(codec = AttachmentCodec())
        Client.register(codec = RemoteAttachmentCodec())
        Client.register(codec = MultiRemoteAttachmentCodec())

        val attachment1 =
            Attachment(
                filename = "test1.txt",
                mimeType = "text/plain",
                data = "hello world".toByteStringUtf8(),
            )

        val attachment2 =
            Attachment(
                filename = "test2.txt",
                mimeType = "text/plain",
                data = "hello world".toByteStringUtf8(),
            )

        val attachmentCodec = AttachmentCodec()
        val remoteAttachmentInfos: MutableList<RemoteAttachmentInfo> = ArrayList()

        for (attachment: Attachment in listOf(attachment1, attachment2)) {
            val encodedBytes = attachmentCodec.encode(attachment).toByteArray()
            val encryptedAttachment =
                MultiRemoteAttachmentCodec.encryptBytesForLocalAttachment(
                    encodedBytes,
                    attachment.filename,
                )
            val url = testUploadEncryptedPayload(encryptedAttachment.payload.toByteArray())
            val remoteAttachmentInfo =
                MultiRemoteAttachmentCodec.buildRemoteAttachmentInfo(
                    encryptedAttachment,
                    URL(url),
                )
            remoteAttachmentInfos.add(remoteAttachmentInfo)
        }

        val multiRemoteAttachment =
            MultiRemoteAttachment(remoteAttachments = remoteAttachmentInfos.toList())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }
        runBlocking {
            alixConversation.send(
                content = multiRemoteAttachment,
                options = SendOptions(contentType = ContentTypeMultiRemoteAttachment),
            )
        }

        val messages = runBlocking { alixConversation.messages() }
        assertEquals(messages.size, 2)

        // Below steps outlines how to handle receiving a MultiRemoteAttachment message
        if (messages.size == 2 &&
            messages[0]
                .encodedContent.type.id
                .equals(ContentTypeMultiRemoteAttachment)
        ) {
            val loadedMultiRemoteAttachment: FfiMultiRemoteAttachment = messages[0].content()!!

            val textAttachments: MutableList<Attachment> = ArrayList()

            for (
            remoteAttachment: RemoteAttachment in
            loadedMultiRemoteAttachment.attachments.map { attachment ->
                RemoteAttachment(
                    url = URL(attachment.url),
                    filename = attachment.filename,
                    contentDigest = attachment.contentDigest,
                    nonce = attachment.nonce.toByteString(),
                    scheme = attachment.scheme,
                    salt = attachment.salt.toByteString(),
                    secret = attachment.secret.toByteString(),
                    contentLength = attachment.contentLength?.toInt(),
                )
            }
            ) {
                val url = remoteAttachment.url.toString()
                // Simulate Download
                val encryptedPayload: ByteArray = encryptedPayloadUrls[url]!!
                // Combine encrypted payload with RemoteAttachmentInfo
                val encryptedAttachment: EncryptedEncodedContent =
                    MultiRemoteAttachmentCodec.buildEncryptAttachmentResult(
                        remoteAttachment,
                        encryptedPayload,
                    )
                // Decrypt payload
                val encodedContent: EncodedContent =
                    MultiRemoteAttachmentCodec.decryptAttachment(encryptedAttachment)
                assertEquals(encodedContent.type.id, ContentTypeText.id)
                // Convert EncodedContent to Attachment
                val attachment = attachmentCodec.decode(encodedContent)
                textAttachments.add(attachment)
            }

            assertEquals(textAttachments[0].filename, "test1.txt")
            assertEquals(textAttachments[1].filename, "test2.txt")
        } else {
            AssertionError("expected a MultiRemoteAttachment message")
        }
    }
}
