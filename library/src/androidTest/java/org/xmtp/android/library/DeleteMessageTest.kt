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
import org.xmtp.android.library.codecs.DeletedBy
import org.xmtp.android.library.codecs.DeletedMessage

@RunWith(AndroidJUnit4::class)
class DeleteMessageTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client
    private lateinit var caroClient: Client

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
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        val messageId =
            runBlocking {
                alixGroup.send("Hello, this message will be deleted")
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
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val messageId =
            runBlocking {
                boGroup.send("Hello from Bo")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertTrue(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId) })

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
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val messageId =
            runBlocking {
                alixGroup.send("Hello from Alix")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertFalse(runBlocking { boGroup.isSuperAdmin(boClient.inboxId) })

        assertThrows(XMTPException::class.java) {
            runBlocking {
                boGroup.deleteMessage(messageId)
            }
        }
    }

    @Test
    fun testCannotDeleteAlreadyDeletedMessage() {
        val alixGroup =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        val messageId =
            runBlocking {
                alixGroup.send("Message to delete twice")
            }

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
        }

        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.deleteMessage(messageId)
            }
        }
    }

    @Test
    fun testDeleteMessageInDm() {
        val alixDm =
            runBlocking {
                alixClient.conversations.findOrCreateDm(boClient.inboxId)
            }

        val messageId =
            runBlocking {
                alixDm.send("Hello in DM")
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
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        val conversation: Conversation = Conversation.Group(alixGroup)

        val messageId =
            runBlocking {
                conversation.send("Hello via conversation")
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
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.deleteMessage("0000000000000000000000000000000000000000000000000000000000000000")
            }
        }
    }

    @Test
    fun testReceiverSeesDeletedMessageContentType() {
        val alixGroup =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val originalText = "Test message for deletion verification"
        val messageId =
            runBlocking {
                alixGroup.send(originalText)
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        var boEnrichedMessages = runBlocking { boGroup.enrichedMessages() }
        val boOriginalEnriched = boEnrichedMessages.find { it.id == messageId }
        assertNotNull(boOriginalEnriched)
        assertEquals(originalText, boOriginalEnriched?.content<String>())

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
        }

        runBlocking { boGroup.sync() }

        boEnrichedMessages = runBlocking { boGroup.enrichedMessages() }
        val boEnrichedAfterDeletion = boEnrichedMessages.find { it.id == messageId }

        assertNotNull(boEnrichedAfterDeletion)

        val deletedContent = boEnrichedAfterDeletion?.content<DeletedMessage>()
        assertNotNull(deletedContent)
        assertTrue(deletedContent?.deletedBy is DeletedBy.Sender)

        assertEquals("xmtp.org", boEnrichedAfterDeletion?.contentTypeId?.authorityId)
        assertEquals("deletedMessage", boEnrichedAfterDeletion?.contentTypeId?.typeId)
    }

    @Test
    fun testAdminDeleteShowsAdminDeletedBy() {
        val alixGroup =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId))
            }

        runBlocking { boClient.conversations.sync() }
        val boGroup =
            runBlocking {
                boClient.conversations.listGroups().first { it.id == alixGroup.id }
            }

        val messageId =
            runBlocking {
                boGroup.send("Message from Bo")
            }

        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        assertTrue(runBlocking { alixGroup.isSuperAdmin(alixClient.inboxId) })

        runBlocking {
            alixGroup.deleteMessage(messageId)
            alixGroup.sync()
            boGroup.sync()
        }

        val boEnrichedMessages = runBlocking { boGroup.enrichedMessages() }
        val deletedMessage = boEnrichedMessages.find { it.id == messageId }
        assertNotNull(deletedMessage)

        val deletedContent = deletedMessage?.content<DeletedMessage>()
        assertNotNull(deletedContent)
        assertTrue(deletedContent?.deletedBy is DeletedBy.Admin)
    }
}
