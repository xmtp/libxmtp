package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
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
        val alixClient = fixtures.alixClient
        val alixConversation = runBlocking {
            alixClient.conversations.newConversation(fixtures.bo.walletAddress)
        }

        runBlocking { alixConversation.send(text = "hey alice 2 bob") }

        val readReceipt = ReadReceipt

        runBlocking {
            alixConversation.send(
                content = readReceipt,
                options = SendOptions(contentType = ContentTypeReadReceipt),
            )
        }
        val messages = runBlocking { alixConversation.messages() }
        assertEquals(messages.size, 2)
        if (messages.size == 2) {
            val contentType: String = messages.first().encodedContent.type.typeId
            assertEquals(contentType, "readReceipt")
        }
        val convos = runBlocking { alixClient.conversations.list() }
        assertEquals(
            runBlocking { convos.first().lastMessage() }!!.encodedContent.type.typeId,
            "text"
        )
    }
}
