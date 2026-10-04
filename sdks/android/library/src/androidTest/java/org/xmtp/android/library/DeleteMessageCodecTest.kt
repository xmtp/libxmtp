package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class DeleteMessageCodecTest : BaseInstrumentedTest() {
    private val value = DeleteMessageContent("ab".repeat(32))

    private fun assertDeletedParent(
        messages: List<Message>,
        parentId: String,
        actionId: String,
    ) {
        assertTrue(actionId.isNotEmpty())
        assertNotEquals(parentId, actionId)
        assertFalse(messages.any { it.id == actionId })
        val parent = messages.single { it.id == parentId }
        val content = ((parent.content as SDKMessageContent.Standard).value as MessageContent.DeletedMessage).v1
        assertTrue(content.deletedBy is DeletedBy.Sender)
        assertEquals(ContentTypeId("xmtp.org", "deletedMessage", 1u, 0u), parent.contentType)
    }

    @Test fun testCanUseDeleteMessageCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            fixtures.boClient.conversations().syncAll(null)
            val received = (checkNotNull(fixtures.boClient.conversations().getById(dm.id())) as Conversation.Dm).dm
            val parentId = dm.sendText("Delete this message")
            received.sync()
            val payload = DeleteMessageContent(parentId)
            val codec = DeleteMessageCodec()
            assertEquals(payload, codec.decode(codec.encode(payload)))
            val actionId = dm.send(codec, payload)
            received.sync()
            assertDeletedParent(received.messages(), parentId, actionId)
        }

    @Test fun testDeleteMessageCodecEncodeDecode() {
        val codec = DeleteMessageCodec()
        assertEquals(value, codec.decode(codec.encode(value)))
    }

    @Test fun testDeleteMessageCodecFallback() = assertNull(DeleteMessageCodec().fallback(value))

    @Test fun testDeleteMessageCodecShouldPush() = assertFalse(DeleteMessageCodec().shouldPush(value))

    @Test fun testDeleteMessageCodecContentType() {
        assertEquals(ContentTypeId("xmtp.org", "deleteMessage", 1u, 0u), DeleteMessageCodec().type)
    }

    @Test fun testReceiverCanDecodeDeleteMessageFromListMessages() =
        runBlocking {
            val fixtures = createFixtures()
            val group = fixtures.alixClient.conversations().createGroup(listOf(fixtures.boClient.inboxId()))
            fixtures.boClient.conversations().syncAll(null)
            val received =
                (
                    checkNotNull(
                        fixtures.boClient.conversations().getById(group.id()),
                    ) as Conversation.Group
                ).group
            val parentId = group.sendText("Delete this message")
            received.sync()
            val payload = DeleteMessageContent(parentId)
            val codec = DeleteMessageCodec()
            assertEquals(payload, codec.decode(codec.encode(payload)))
            val actionId = group.send(codec, payload)
            received.sync()
            assertDeletedParent(received.messages(), parentId, actionId)
        }

    @Test fun testDeleteMessageContentTypeInListMessages() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            fixtures.boClient.conversations().syncAll(null)
            val received = (checkNotNull(fixtures.boClient.conversations().getById(dm.id())) as Conversation.Dm).dm
            val parentId = dm.sendText("Delete this message")
            received.sync()
            val payload = DeleteMessageContent(parentId)
            val codec = DeleteMessageCodec()
            val encoded = codec.encode(payload)
            assertEquals(codec.type, encoded.type)
            assertEquals(payload, codec.decode(encoded))
            val actionId = dm.send(codec, payload)
            received.sync()
            assertDeletedParent(received.messages(), parentId, actionId)
        }
}
