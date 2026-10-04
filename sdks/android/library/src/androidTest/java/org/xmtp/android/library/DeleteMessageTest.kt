package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
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
    private lateinit var caroClient: SDKClient

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testSenderCanDeleteOwnMessage() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        val messageId =
            runBlocking {
                alixGroup.sendText("Hello, this message will be deleted")
            }

        runBlocking { alixGroup.sync() }
        var messages = runBlocking { alixGroup.messages() }
        assertTrue(messages.any { it.id == messageId })

        val deletionMessageId =
            runBlocking {
                alixGroup.deleteMessage(messageId)
            }
        assertNotNull(deletionMessageId)

        runBlocking { alixGroup.sync() }
        messages = runBlocking { alixGroup.messages() }
        assertTrue(messages.any { it.id == deletionMessageId })
    }

    @Test
    fun testSuperAdminCanDeleteOthersMessage() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        runBlocking { boClient.conversations().sync() }
        val boGroup =
            runBlocking {
                boClient.conversations().listGroups(null).first { it.id() == alixGroup.id() }
            }

        val messageId =
            runBlocking {
                boGroup.sendText("Hello from Bo")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertTrue(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId()) })

        val deletionMessageId =
            runBlocking {
                alixGroup.deleteMessage(messageId)
            }
        assertNotNull(deletionMessageId)
    }

    @Test
    fun testRegularUserCannotDeleteOthersMessage() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        runBlocking { boClient.conversations().sync() }
        val boGroup =
            runBlocking {
                boClient.conversations().listGroups(null).first { it.id() == alixGroup.id() }
            }

        val messageId =
            runBlocking {
                alixGroup.sendText("Hello from Alix")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertFalse(runBlocking { boGroup.isSuperAdmin(boClient.inboxId()) })

        assertThrows(XmtpException::class.java) {
            runBlocking {
                boGroup.deleteMessage(messageId)
            }
        }
    }

    @Test
    fun testCannotDeleteAlreadyDeletedMessage() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        val messageId =
            runBlocking {
                alixGroup.sendText("Message to delete twice")
            }

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
        }

        assertThrows(XmtpException::class.java) {
            runBlocking {
                alixGroup.deleteMessage(messageId)
            }
        }
    }

    @Test
    fun testDeleteMessageInDm() {
        val alixDm =
            runBlocking {
                alixClient.conversations().createDm(boClient.inboxId())
            }

        val messageId =
            runBlocking {
                alixDm.sendText("Hello in DM")
            }

        runBlocking { alixDm.sync() }
        var messages = runBlocking { alixDm.messages() }
        assertTrue(messages.any { it.id == messageId })

        val deletionMessageId =
            runBlocking {
                alixDm.deleteMessage(messageId)
            }
        assertNotNull(deletionMessageId)

        runBlocking { alixDm.sync() }
        messages = runBlocking { alixDm.messages() }
        assertTrue(messages.any { it.id == deletionMessageId })
    }

    @Test
    fun testDeleteMessageViaConversation() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        val conversation: Conversation = Conversation.Group(alixGroup)

        val messageId =
            runBlocking {
                conversation.sendText("Hello via conversation")
            }

        val deletionMessageId =
            runBlocking {
                conversation.deleteMessage(messageId)
            }
        assertNotNull(deletionMessageId)
    }

    @Test
    fun testDeleteMessageWithInvalidId() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        assertThrows(XmtpException::class.java) {
            runBlocking {
                alixGroup.deleteMessage("0000000000000000000000000000000000000000000000000000000000000000")
            }
        }
    }

    @Test
    fun testReceiverSeesDeletedMessageContentType() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        runBlocking { boClient.conversations().sync() }
        val boGroup =
            runBlocking {
                boClient.conversations().listGroups(null).first { it.id() == alixGroup.id() }
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

    @Test
    fun testAdminDeleteShowsAdminDeletedBy() {
        val alixGroup =
            runBlocking {
                alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            }

        runBlocking { boClient.conversations().sync() }
        val boGroup =
            runBlocking {
                boClient.conversations().listGroups(null).first { it.id() == alixGroup.id() }
            }

        val messageId =
            runBlocking {
                boGroup.sendText("Message from Bo")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertTrue(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId()) })

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
            boGroup.sync()
        }

        val boEnrichedMessages = runBlocking { boGroup.messages() }
        val deletedMessage = boEnrichedMessages.find { it.id == messageId }
        assertNotNull(deletedMessage)

        val deletedContent = deleted(deletedMessage)
        assertNotNull(deletedContent)
        assertTrue(deletedContent?.deletedBy is DeletedBy.Admin)
    }
}
