package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class DeleteMessageCodecTest : BaseInstrumentedTest() {
    private val value = DeleteMessageContent("ab".repeat(32))

    @Test fun testCanUseDeleteMessageCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val id = dm.send(DeleteMessageCodec(), value)
            val message = dm.messages().single { it.id == id }
            assertEquals(value, DeleteMessageCodec().decode(checkNotNull(message.encoded)))
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
            val id = group.send(DeleteMessageCodec(), value)
            received.sync()
            val message = received.messages().single { it.id == id }
            assertEquals(value, DeleteMessageCodec().decode(checkNotNull(message.encoded)))
        }

    @Test fun testDeleteMessageContentTypeInListMessages() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val id = dm.send(DeleteMessageCodec(), value)
            val message = dm.messages().single { it.id == id }
            assertEquals(DeleteMessageCodec().type, message.contentType)
            assertEquals(value, DeleteMessageCodec().decode(checkNotNull(message.encoded)))
        }
}
