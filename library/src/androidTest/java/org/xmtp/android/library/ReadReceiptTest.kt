package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReadReceipt
import org.xmtp.android.library.codecs.ReadReceipt
import org.xmtp.android.library.codecs.ReadReceiptCodec
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class ReadReceiptTest {

    @Test
    fun testCanUseReadReceiptCodec() {
        Client.register(codec = ReadReceiptCodec())

        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)

        aliceConversation.send(text = "hey alice 2 bob")

        val readReceipt = ReadReceipt(timestamp = "2019-09-26T07:58:30.996+0200")

        aliceConversation.send(
            content = readReceipt,
            options = SendOptions(contentType = ContentTypeReadReceipt),
        )
        val messages = aliceConversation.messages()
        assertEquals(messages.size, 2)
        if (messages.size == 2) {
            val content: ReadReceipt? = messages.first().content()
            assertEquals("2019-09-26T07:58:30.996+0200", content?.timestamp)
        }
    }
}
