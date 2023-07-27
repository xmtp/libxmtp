package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReply
import org.xmtp.android.library.codecs.ContentTypeText
import org.xmtp.android.library.codecs.Reply
import org.xmtp.android.library.codecs.ReplyCodec
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class ReplyTest {

    @Test
    fun testCanUseReplyCodec() {
        Client.register(codec = ReplyCodec())

        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)

        aliceConversation.send(text = "hey alice 2 bob")

        val messageToReact = aliceConversation.messages()[0]

        val attachment = Reply(
            reference = messageToReact.id,
            content = "Hello",
            contentType = ContentTypeText
        )

        aliceConversation.send(
            content = attachment,
            options = SendOptions(contentType = ContentTypeReply),
        )
        val messages = aliceConversation.messages()
        assertEquals(messages.size, 2)
        if (messages.size == 2) {
            val content: Reply? = messages.first().content()
            assertEquals("Hello", content?.content)
            assertEquals(messageToReact.id, content?.reference)
            assertEquals(ContentTypeText, content?.contentType)
        }
    }
}
