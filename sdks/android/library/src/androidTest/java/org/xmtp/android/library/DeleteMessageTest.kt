package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class DeleteMessageTest : BaseInstrumentedTest() {
    private fun text(message: Message?): String? =
        ((message?.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private fun deleted(message: Message?): DeletedMessage? =
        ((message?.content as? SDKMessageContent.Standard)?.value as? MessageContent.DeletedMessage)?.v1

    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: SDKClient
    private lateinit var boClient: SDKClient

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    @Test
    fun testReceiverSeesDeletedMessageContentType() {
        val alixGroup =
            runBlocking {
                alixClient.conversations.createGroup(listOf(boClient.inboxId()))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups(null).first { it.id() == alixGroup.id() }
            }

        val originalText = "Test message for deletion verification"
        val messageId =
            runBlocking {
                alixGroup.sendText(originalText)
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        var boEnrichedMessages = runBlocking { boGroup.messages() }
        val boOriginalEnriched = boEnrichedMessages.find { it.id == messageId }
        assertNotNull(boOriginalEnriched)
        assertEquals(originalText, text(boOriginalEnriched))

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
        }

        runBlocking { boGroup.sync() }

        boEnrichedMessages = runBlocking { boGroup.messages() }
        val boEnrichedAfterDeletion = boEnrichedMessages.find { it.id == messageId }

        assertNotNull(boEnrichedAfterDeletion)

        val deletedContent = deleted(boEnrichedAfterDeletion)
        assertNotNull(deletedContent)
        assertTrue(deletedContent?.deletedBy is DeletedBy.Sender)

        assertEquals("xmtp.org", boEnrichedAfterDeletion?.contentType?.authorityId)
        assertEquals("deletedMessage", boEnrichedAfterDeletion?.contentType?.typeId)
    }
}
