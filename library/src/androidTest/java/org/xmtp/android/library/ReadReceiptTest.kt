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
        val aliceClient = fixtures.alixClient
        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(fixtures.bo.walletAddress)
        }

        runBlocking { aliceConversation.send(text = "hey alice 2 bob") }

        val readReceipt = ReadReceipt

        runBlocking {
            aliceConversation.send(
                content = readReceipt,
                options = SendOptions(contentType = ContentTypeReadReceipt),
            )
        }
        val messages = runBlocking { aliceConversation.messages() }
        assertEquals(messages.size, 3)
        if (messages.size == 3) {
            val contentType: String = messages.first().encodedContent.type.typeId
            assertEquals(contentType, "readReceipt")
        }
    }
}
