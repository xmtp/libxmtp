package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class ReadReceiptTest : BaseInstrumentedTest() {
    @Test fun testCanUseReadReceiptCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val text = dm.sendText("hey alice 2 bob")
            val receipt = dm.sendReadReceipt()
            val message = dm.messages().single { it.id == receipt }
            assertEquals(MessageContent.ReadReceipt, (message.content as SDKMessageContent.Standard).value)
            assertEquals(ReadReceiptCodec().type, message.contentType)
            assertEquals(text, checkNotNull(dm.lastMessage()).id)
        }
}
