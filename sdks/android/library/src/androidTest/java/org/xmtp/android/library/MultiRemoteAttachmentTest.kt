package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class MultiRemoteAttachmentTest : BaseInstrumentedTest() {
    @Test fun testCanUseMultiRemoteAttachmentCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val encoded = mutableMapOf<String, Pair<EncodedContent, EncryptedEncodedContent>>()
            val attachments =
                listOf("test1.txt", "test2.txt").mapIndexed { index, filename ->
                    val value = Attachment(filename, "text/plain", "hello world".toByteArray())
                    val body = AttachmentCodec().encode(value)
                    val encrypted = encryptBytes(body.content)
                    val url = "https://attachments.example.test/$index"
                    encoded[url] = body to encrypted
                    remoteAttachmentFromEncrypted(url, encrypted, filename)
                }
            val value = MultiRemoteAttachment(attachments)
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val id = dm.sendMultiRemoteAttachment(value)
            val received =
                (
                    (
                        dm
                            .messages()
                            .single {
                                it.id == id
                            }.content as SDKMessageContent.Standard
                    ).value as MessageContent.MultiRemoteAttachment
                ).v1
            assertEquals(value, received)
            val restored =
                received.attachments.map { remote ->
                    val (body, encrypted) = checkNotNull(encoded[remote.url])
                    val plain =
                        decryptBytes(
                            encrypted.ciphertext,
                            EncryptionKeys(
                                remote.secret,
                                remote.salt,
                                remote.nonce,
                                remote.contentDigest,
                                checkNotNull(remote.contentLength).toULong(),
                            ),
                        )
                    assertArrayEquals(body.content, plain)
                    AttachmentCodec().decode(body.copy(content = plain))
                }
            assertEquals(listOf("test1.txt", "test2.txt"), restored.map { it.filename })
            assertTrue(restored.all { it.content.contentEquals("hello world".toByteArray()) })
        }
}
