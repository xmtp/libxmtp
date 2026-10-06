package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class DeleteMessageCodecTest : BaseInstrumentedTest() {
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
}
