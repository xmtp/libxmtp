package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class NumberCodec : ContentCodec<Double> {
    override val type = ContentTypeId("example.com", "number", 1u, 1u)

    override fun encode(value: Double) = EncodedContent(type, content = value.toString().toByteArray())

    override fun decode(encoded: EncodedContent) = encoded.content.toString(Charsets.UTF_8).toDouble()

    override fun shouldPush(value: Double) = false

    override fun fallback(value: Double) = "Error: This app does not support numbers."
}

class CodecTest : BaseInstrumentedTest() {
    // verifies: CTYPE-007, CTYPE-017
    @Test fun testCanRoundTripWithCustomContentType() =
        runBlocking {
            val codec = NumberCodec()
            val sender = createClient(createWallet(), codecs = listOf(codec))
            val receiver = createClient(createWallet(), codecs = listOf(codec))
            val dm = sender.conversations.createDm(receiver.inboxId())
            val id = dm.send(codec, 3.14)
            receiver.conversations.syncAll(null)
            val message = checkNotNull(receiver.conversations.getMessageById(id))
            assertEquals(3.14, (message.content as SDKMessageContent.Custom).value)
            assertEquals(codec.type, message.contentType)
            assertEquals("Error: This app does not support numbers.", message.fallback)
            assertFalse(codec.shouldPush(3.14))
        }
}
