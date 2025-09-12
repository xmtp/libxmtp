package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeReaction
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionCodec
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder

@RunWith(AndroidJUnit4::class)
class DecodedMessageV2Test {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        fixtures = fixtures()
        alixWallet = fixtures.alixAccount
        alix = fixtures.alix
        boWallet = fixtures.boAccount
        bo = fixtures.bo
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient

        Client.register(codec = ReactionCodec())
    }

    @Test
    fun testCanRetrieveMessagesV2FromGroup() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        runBlocking {
            boGroup.send("Hello from Bo")
            alixGroup.send("Hello from Alix")
            boGroup.send("Second message from Bo")
        }

        val messagesV2 = runBlocking {
            boGroup.messagesV2()
        }

        // Groups include a GroupUpdated message when members are added
        assertEquals(4, messagesV2.size)
        assertEquals("Second message from Bo", messagesV2[0].content<String>())
        assertEquals("Hello from Alix", messagesV2[1].content<String>())
        assertEquals("Hello from Bo", messagesV2[2].content<String>())
        // The last message is the GroupUpdated message from group creation
        assertNotNull(messagesV2[3].content<Any>())
    }

    @Test
    fun testCanRetrieveMessagesV2FromDm() {
        val boDm = runBlocking {
            boClient.conversations.newConversation(alixClient.inboxId)
        }
        runBlocking {
            alixClient.conversations.sync()
        }
        val alixDm = runBlocking { alixClient.conversations.listDms().first() }

        runBlocking {
            boDm.send("Hello from Bo")
            alixDm.send("Hello from Alix")
            boDm.send("Second message from Bo")
        }

        val messagesV2 = runBlocking {
            boDm.messagesV2()
        }

        // DMs include a GroupUpdated message when the conversation is created
        assertEquals(4, messagesV2.size)
        assertEquals("Second message from Bo", messagesV2[0].content<String>())
        assertEquals("Hello from Alix", messagesV2[1].content<String>())
        assertEquals("Hello from Bo", messagesV2[2].content<String>())
        // The last message is the GroupUpdated message from DM creation
        assertNotNull(messagesV2[3].content<Any>())
    }

    @Test
    fun testMessagesV2Pagination() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }

        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }

        runBlocking {
            for (i in 1..10) {
                boGroup.send("Message $i from Bo")
            }
        }

        val limitedMessages = runBlocking {
            boGroup.messagesV2(limit = 5)
        }
        assertEquals(5, limitedMessages.size)

        val beforeMessages = runBlocking {
            boGroup.messagesV2(beforeNs = limitedMessages[2].sentAtNs)
        }
        assertTrue(beforeMessages.all { it.sentAtNs < limitedMessages[2].sentAtNs })

        val afterMessages = runBlocking {
            boGroup.messagesV2(afterNs = limitedMessages[2].sentAtNs)
        }
        assertTrue(afterMessages.all { it.sentAtNs > limitedMessages[2].sentAtNs })
    }

    @Test
    fun testMessagesV2SortDirection() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }

        runBlocking {
            boGroup.send("First message")
            Thread.sleep(100)
            boGroup.send("Second message")
            Thread.sleep(100)
            boGroup.send("Third message")
        }

        val descendingMessages = runBlocking {
            boGroup.messagesV2(direction = DecodedMessage.SortDirection.DESCENDING)
        }
        // Skip GroupUpdated message, check text messages
        assertEquals("Third message", descendingMessages[0].content<String>())
        assertEquals("Second message", descendingMessages[1].content<String>())
        assertEquals("First message", descendingMessages[2].content<String>())

        val ascendingMessages = runBlocking {
            boGroup.messagesV2(direction = DecodedMessage.SortDirection.ASCENDING)
        }
        // First message is GroupUpdated, then text messages
        assertNotNull(ascendingMessages[0].content<Any>()) // GroupUpdated
        assertEquals("First message", ascendingMessages[1].content<String>())
        assertEquals("Second message", ascendingMessages[2].content<String>())
        assertEquals("Third message", ascendingMessages[3].content<String>())
    }

    @Test
    fun testMessagesV2DeliveryStatus() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }

        runBlocking {
            boGroup.send("Published message")
            boGroup.prepareMessage("Unpublished message")
        }

        val allMessages = runBlocking {
            boGroup.messagesV2(deliveryStatus = DecodedMessage.MessageDeliveryStatus.ALL)
        }
        // 2 user messages + 1 GroupUpdated message
        assertEquals(3, allMessages.size)

        val publishedMessages = runBlocking {
            boGroup.messagesV2(deliveryStatus = DecodedMessage.MessageDeliveryStatus.PUBLISHED)
        }
        // 1 published user message + 1 GroupUpdated message
        assertEquals(2, publishedMessages.size)
        assertEquals("Published message", publishedMessages[0].content<String>())

        val unpublishedMessages = runBlocking {
            boGroup.messagesV2(deliveryStatus = DecodedMessage.MessageDeliveryStatus.UNPUBLISHED)
        }
        assertEquals(1, unpublishedMessages.size)
        assertEquals("Unpublished message", unpublishedMessages[0].content<String>())
    }

    @Test
    fun testMessagesV2CustomContentTypes() = runBlocking {
        val group = alixClient.conversations.newGroup(listOf(boClient.inboxId))

        Client.register(codec = NumberCodec())

        val myNumber = 3.14

        group.send(
            content = myNumber,
            options = SendOptions(contentType = NumberCodec().contentType),
        )

        val messages = group.messagesV2()
        val content: Double? = messages[0].content()
        assertEquals(myNumber, content)
    }

    @Test
    fun testMessagesV2IncludeReactions() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        runBlocking {
            val messageId = boGroup.send("Hello with reactions")
            boGroup.sync()
            alixGroup.sync()

            alixGroup.send(
                content = Reaction(
                    reference = messageId,
                    action = ReactionAction.Added,
                    content = "👍",
                    schema = ReactionSchema.Unicode
                ),
                options = SendOptions(contentType = ContentTypeReaction)
            )

            boGroup.send(
                content = Reaction(
                    reference = messageId,
                    action = ReactionAction.Added,
                    content = "❤️",
                    schema = ReactionSchema.Unicode
                ),
                options = SendOptions(contentType = ContentTypeReaction)
            )
            boGroup.sync()
            alixGroup.sync()
        }

        val messagesV2 = runBlocking {
            boGroup.messagesV2()
        }

        val messageWithReactions =
            messagesV2.find { it.content<String>() == "Hello with reactions" }
        assertNotNull(messageWithReactions)
        assertTrue(messageWithReactions!!.hasReactions)
        assertEquals(2, messageWithReactions.reactionCount.toInt())
        assertEquals(2, messageWithReactions.reactions.size)

        val reactionContents = messageWithReactions.reactions.mapNotNull {
            it.content<Reaction>()?.content
        }.sorted()
        assertEquals(listOf("❤️", "👍"), reactionContents)
    }

    @Test
    fun testReactionCountAccuracy() {
        val boGroup = runBlocking {
            boClient.conversations.newGroup(listOf(alixClient.inboxId))
        }
        runBlocking {
            alixClient.conversations.sync()
            boGroup.sync()
        }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        runBlocking {
            val messageId = boGroup.send("Test reaction count")
            boGroup.sync()
            alixGroup.sync()

            for (i in 1..5) {
                alixGroup.send(
                    content = Reaction(
                        reference = messageId,
                        action = ReactionAction.Added,
                        content = "emoji$i",
                        schema = ReactionSchema.Unicode
                    ),
                    options = SendOptions(contentType = ContentTypeReaction)
                )
            }
            boGroup.sync()
        }

        val messagesV2 = runBlocking {
            boGroup.messagesV2()
        }

        val message = messagesV2.find { it.content<String>() == "Test reaction count" }
        assertNotNull(message)
        assertEquals(5, message!!.reactionCount.toInt())
        assertEquals(5, message.reactions.size)
    }
}
