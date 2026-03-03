package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeEditMessageRequest
import org.xmtp.android.library.codecs.EditMessageCodec
import org.xmtp.android.library.codecs.EditMessageRequest

@RunWith(AndroidJUnit4::class)
class EditMessageCodecTest : BaseInstrumentedTest() {
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
    fun testCanUseEditMessageCodec() {
        Client.register(codec = EditMessageCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val editRequest =
            EditMessageRequest(
                messageId = "test-message-id-123",
                editedContent = null,
            )

        runBlocking {
            alixConversation.send(
                content = editRequest,
                options = SendOptions(contentType = ContentTypeEditMessageRequest),
            )
        }

        val messages = runBlocking { alixConversation.messages() }

        assertEquals(2, messages.size)

        if (messages.size == 2) {
            val content: EditMessageRequest? = messages.first().content()
            assertNotNull(content)
            assertEquals("test-message-id-123", content?.messageId)
        }
    }

    @Test
    fun testEditMessageCodecEncodeDecode() {
        val codec = EditMessageCodec()

        val original = EditMessageRequest(messageId = "message-to-edit", editedContent = null)
        val encoded = codec.encode(original)
        val decoded = codec.decode(encoded)

        assertEquals(original.messageId, decoded.messageId)
    }

    @Test
    fun testEditMessageCodecFallback() {
        val codec = EditMessageCodec()
        val content = EditMessageRequest(messageId = "any-id", editedContent = null)
        val fallback = codec.fallback(content)
        assertNull(fallback)
    }

    @Test
    fun testEditMessageCodecShouldPush() {
        val codec = EditMessageCodec()
        val content = EditMessageRequest(messageId = "any-id", editedContent = null)
        val shouldPush = codec.shouldPush(content)
        assertFalse(shouldPush)
    }

    @Test
    fun testEditMessageCodecContentType() {
        val codec = EditMessageCodec()
        assertEquals(ContentTypeEditMessageRequest, codec.contentType)
        assertEquals("xmtp.org", codec.contentType.authorityId)
        assertEquals("editMessage", codec.contentType.typeId)
        assertEquals(1, codec.contentType.versionMajor)
        assertEquals(0, codec.contentType.versionMinor)
    }

    @Test
    fun testReceiverCanDecodeEditMessageFromListMessages() {
        Client.register(codec = EditMessageCodec())

        val alixGroup =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val editRequest =
            EditMessageRequest(
                messageId = "message-id-to-edit-456",
                editedContent = null,
            )

        runBlocking {
            alixGroup.send(
                content = editRequest,
                options = SendOptions(contentType = ContentTypeEditMessageRequest),
            )
        }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        val boMessages = runBlocking { boGroup.messages() }
        val editMessage =
            boMessages.find {
                it.encodedContent.type.typeId == "editMessage"
            }

        assertNotNull(editMessage)
        val content: EditMessageRequest? = editMessage?.content()
        assertNotNull(content)
        assertEquals("message-id-to-edit-456", content?.messageId)
    }

    @Test
    fun testEditMessageContentTypeInListMessages() {
        Client.register(codec = EditMessageCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val editRequest = EditMessageRequest(messageId = "test-msg-789", editedContent = null)

        runBlocking {
            alixConversation.send(
                content = editRequest,
                options = SendOptions(contentType = ContentTypeEditMessageRequest),
            )
        }

        val messages = runBlocking { alixConversation.messages() }
        val editMsg =
            messages.find {
                it.encodedContent.type.typeId == "editMessage"
            }

        assertNotNull(editMsg)
        assertEquals("xmtp.org", editMsg?.encodedContent?.type?.authorityId)
        assertEquals("editMessage", editMsg?.encodedContent?.type?.typeId)

        val decoded: EditMessageRequest? = editMsg?.content()
        assertNotNull(decoded)
        assertEquals("test-msg-789", decoded?.messageId)
    }
}
