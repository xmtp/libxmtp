package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReplyTest : BaseInstrumentedTest() {
    @Test fun testCanUseReplyCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val parent = dm.sendText("hey alice 2 bob")
            val value = ReplyContent(parent, null, TextCodec().encode("Hello"))
            val id = dm.send(ReplyCodec(), value)
            val message = dm.messages().single { it.id == id }
            val reply = (message.content as SDKMessageContent.Standard).value as MessageContent.Reply
            assertEquals(parent, reply.referenceId)
            assertEquals(MessageBody.Text("Hello"), reply.body)
            assertEquals(value, ReplyCodec().decode(checkNotNull(message.encoded)))
        }

    private suspend fun reply(
        text: String,
        missing: Boolean = false,
    ): Message {
        val fixtures = createFixtures()
        val group = fixtures.alixClient.conversations().createGroup(listOf(fixtures.boClient.inboxId()))
        fixtures.boClient.conversations().syncAll(null)
        val receiver = (checkNotNull(fixtures.boClient.conversations().getById(group.id())) as Conversation.Group).group
        val parent = if (missing) "ab".repeat(32) else group.sendText("Original message")
        receiver.sync()
        val id = receiver.sendReply(parent, null, TextCodec().encode(text))
        group.sync()
        return group.messages().single { it.id == id }
    }

    @Test fun testMessagesV2WithReply() =
        runBlocking {
            val message = reply("This is a reply")
            assertEquals(MessageBody.Text("This is a reply"), (message.replyContent as SDKReplyContent.Standard).value)
            assertEquals(MessageBody.Text("Original message"), checkNotNull(message.inReplyTo).content)
        }

    @Test fun testMessagesV2WithReplyToDeletedMessage() =
        runBlocking {
            val message = reply("Reply to deleted", true)
            assertEquals(MessageBody.Text("Reply to deleted"), (message.replyContent as SDKReplyContent.Standard).value)
            assertNull(message.inReplyTo)
        }

    @Test fun testMessagesV2WithReplyIncludesReferencedMessage() =
        runBlocking {
            val message = reply("Text reply")
            assertEquals(MessageBody.Text("Text reply"), (message.replyContent as SDKReplyContent.Standard).value)
            assertNotNull(message.inReplyTo)
            assertEquals(checkNotNull(message.inReplyTo).id, checkNotNull(message.parent()).id)
        }
}
