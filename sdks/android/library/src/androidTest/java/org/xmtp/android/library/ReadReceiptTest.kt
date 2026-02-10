package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReadReceipt
import org.xmtp.android.library.codecs.ReadReceipt
import org.xmtp.android.library.codecs.ReadReceiptCodec

@RunWith(AndroidJUnit4::class)
class ReadReceiptTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client

    @org.junit.Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    @Test
    fun testCanUseReadReceiptCodec() {
        Client.register(codec = ReadReceiptCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
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
        assertEquals(messages.size, 3)
        if (messages.size == 3) {
            val contentType: String =
                messages
                    .first()
                    .encodedContent.type.typeId
            assertEquals(contentType, "readReceipt")
        }
        val convos = runBlocking { alixClient.conversations.list() }
        assertEquals(
            runBlocking { convos.first().lastMessage() }!!.encodedContent.type.typeId,
            "text",
        )
    }
}
