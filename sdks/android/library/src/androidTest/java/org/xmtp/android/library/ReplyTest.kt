package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReplyTest : BaseInstrumentedTest() {
    @Test fun testMessagesV2WithReplyIncludesReferencedMessage() =
        runBlocking {
            val fixtures = createFixtures()
            val group = fixtures.alixClient.conversations.createGroup(listOf(fixtures.boClient.inboxId()))
            fixtures.boClient.conversations.syncAll(null)
            val receiver =
                (checkNotNull(fixtures.boClient.conversations.getById(group.id())) as Conversation.Group).group
            val parent = group.sendText("Original message")
            receiver.sync()
            val id = receiver.sendReply(parent, null, TextCodec().encode("Text reply"))
            group.sync()
            val message = group.messages().single { it.id == id }
            assertEquals(MessageBody.Text("Text reply"), (message.replyContent as SDKReplyContent.Standard).value)
            assertEquals(MessageBody.Text("Original message"), checkNotNull(message.inReplyTo).content)
            assertEquals(checkNotNull(message.inReplyTo).id, checkNotNull(message.parent()).id)
        }
}
