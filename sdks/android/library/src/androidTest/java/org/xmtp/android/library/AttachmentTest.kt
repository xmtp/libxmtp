package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class AttachmentTest : BaseInstrumentedTest() {
    @Test fun testCanUseAttachmentCodec() =
        runBlocking {
            val fixtures = createFixtures()
            val attachment = Attachment("test.txt", "text/plain", "hello world".toByteArray())
            val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
            val id = dm.sendAttachment(attachment)
            val message = dm.messages().single { it.id == id }
            val received = ((message.content as SDKMessageContent.Standard).value as MessageContent.Attachment).v1
            assertEquals(attachment.filename, received.filename)
            assertEquals(attachment.mimeType, received.mimeType)
            assertArrayEquals(attachment.content, received.content)
            assertEquals(attachment, AttachmentCodec().decode(AttachmentCodec().encode(attachment)))
        }
}
