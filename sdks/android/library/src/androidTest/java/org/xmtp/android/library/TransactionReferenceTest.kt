package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class TransactionReferenceTest : BaseInstrumentedTest() {
    @Test fun testCanUseTransactionReferenceCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val value =
                TransactionReference(
                    "eip155",
                    "0x1",
                    "0xabc123",
                    TransactionMetadata("transfer", "ETH", 0.05, 18u, "0xAlice", "0xBob"),
                )
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val id = dm.sendTransactionReference(value)
            val message = dm.messages().single { it.id == id }
            assertEquals(
                value,
                ((message.content as SDKMessageContent.Standard).value as MessageContent.TransactionReference).v1,
            )
            val codec = TransactionReferenceCodec()
            assertEquals(value, codec.decode(codec.encode(value)))
        }
}
