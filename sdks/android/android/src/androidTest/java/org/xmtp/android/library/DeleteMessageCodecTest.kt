package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeDeleteMessageRequest
import org.xmtp.android.library.codecs.DeleteMessageCodec
import org.xmtp.android.library.codecs.DeleteMessageRequest

@RunWith(AndroidJUnit4::class)
class DeleteMessageCodecTest : BaseInstrumentedTest() {
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

    @Test
    fun testCanUseDeleteMessageCodec() {
        Client.register(codec = DeleteMessageCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val deleteRequest =
            DeleteMessageRequest(
                messageId = "test-message-id-123",
            )

        runBlocking {
            alixConversation.send(
                content = deleteRequest,
                options = SendOptions(contentType = ContentTypeDeleteMessageRequest),
            )
        }

        val messages = runBlocking { alixConversation.messages() }

        assertEquals(2, messages.size)

        if (messages.size == 2) {
            val content: DeleteMessageRequest? = messages.first().content()
            assertNotNull(content)
            assertEquals("test-message-id-123", content?.messageId)
        }
    }

    @Test
    fun testDeleteMessageCodecEncodeDecode() {
        val codec = DeleteMessageCodec()

        val original = DeleteMessageRequest(messageId = "message-to-delete")
        val encoded = codec.encode(original)
        val decoded = codec.decode(encoded)

        assertEquals(original, decoded)
        assertEquals(original.messageId, decoded.messageId)
    }

    @Test
    fun testDeleteMessageCodecFallback() {
        val codec = DeleteMessageCodec()
        val content = DeleteMessageRequest(messageId = "any-id")
        val fallback = codec.fallback(content)
        assertNull(fallback)
    }

    @Test
    fun testDeleteMessageCodecShouldPush() {
        val codec = DeleteMessageCodec()
        val content = DeleteMessageRequest(messageId = "any-id")
        val shouldPush = codec.shouldPush(content)
        assertFalse(shouldPush)
    }

    @Test
    fun testDeleteMessageCodecContentType() {
        val codec = DeleteMessageCodec()
        assertEquals(ContentTypeDeleteMessageRequest, codec.contentType)
        assertEquals("xmtp.org", codec.contentType.authorityId)
        assertEquals("deleteMessage", codec.contentType.typeId)
        assertEquals(1, codec.contentType.versionMajor)
        assertEquals(0, codec.contentType.versionMinor)
    }

    @Test
    fun testReceiverCanDecodeDeleteMessageFromListMessages() {
        Client.register(codec = DeleteMessageCodec())

        val alixGroup =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val deleteRequest =
            DeleteMessageRequest(
                messageId = "message-id-to-delete-456",
            )

        runBlocking {
            alixGroup.send(
                content = deleteRequest,
                options = SendOptions(contentType = ContentTypeDeleteMessageRequest),
            )
        }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        // Receiver reads using messages() - not enrichedMessages()
        val boMessages = runBlocking { boGroup.messages() }
        val deleteMessage =
            boMessages.find {
                it.encodedContent.type.typeId == "deleteMessage"
            }

        assertNotNull(deleteMessage)
        val content: DeleteMessageRequest? = deleteMessage?.content()
        assertNotNull(content)
        assertEquals("message-id-to-delete-456", content?.messageId)
    }

    @Test
    fun testDeleteMessageContentTypeInListMessages() {
        Client.register(codec = DeleteMessageCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val deleteRequest = DeleteMessageRequest(messageId = "test-msg-789")

        runBlocking {
            alixConversation.send(
                content = deleteRequest,
                options = SendOptions(contentType = ContentTypeDeleteMessageRequest),
            )
        }

        // Using messages() API to verify content type is preserved
        val messages = runBlocking { alixConversation.messages() }
        val deleteMsg =
            messages.find {
                it.encodedContent.type.typeId == "deleteMessage"
            }

        assertNotNull(deleteMsg)
        assertEquals("xmtp.org", deleteMsg?.encodedContent?.type?.authorityId)
        assertEquals("deleteMessage", deleteMsg?.encodedContent?.type?.typeId)

        // Verify we can decode the content
        val decoded: DeleteMessageRequest? = deleteMsg?.content()
        assertNotNull(decoded)
        assertEquals("test-msg-789", decoded?.messageId)
    }
}
