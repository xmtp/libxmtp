package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeTransactionReference
import org.xmtp.android.library.codecs.TransactionReference
import org.xmtp.android.library.codecs.TransactionReferenceCodec

@RunWith(AndroidJUnit4::class)
class TransactionReferenceTest {

    @Test
    fun testCanUseTransactionReferenceCodec() {
        Client.register(codec = TransactionReferenceCodec())

        val fixtures = fixtures()
        val aliceClient = fixtures.alixClient
        val bobInboxId = fixtures.boClient.inboxId

        val aliceConversation = runBlocking {
            aliceClient.conversations.newConversation(bobInboxId)
        }

        val txRef = TransactionReference(
            namespace = "eip155",
            networkId = "0x1",
            reference = "0xabc123",
            metadata = TransactionReference.Metadata(
                transactionType = "transfer",
                currency = "ETH",
                amount = 0.05,
                decimals = 18u,
                fromAddress = "0xAlice",
                toAddress = "0xBob"
            )
        )

        runBlocking {
            aliceConversation.send(
                content = txRef,
                options = SendOptions(contentType = ContentTypeTransactionReference),
            )
        }

        val messages = runBlocking { aliceConversation.messages() }

        assertEquals(2, messages.size)

        if (messages.size == 2) {
            val content: TransactionReference? = messages.first().content()
            assertEquals("0xabc123", content?.reference)
            assertEquals("ETH", content?.metadata?.currency)
            assertEquals("0x1", content?.networkId)
            assertEquals("transfer", content?.metadata?.transactionType)
        }
    }
}
