package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import kotlin.time.Duration.Companion.seconds

@RunWith(AndroidJUnit4::class)
class ConversationsTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var davonWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var davon: PrivateKey
    private lateinit var davonClient: Client
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
        davonWallet = fixtures.davonAccount
        davon = fixtures.davon

        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
        davonClient = fixtures.davonClient
    }

    @Test
    fun testsCanFindConversationByTopic() {
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }

        val sameDm = runBlocking { boClient.conversations.findConversationByTopic(dm.topic) }
        val sameGroup = runBlocking { boClient.conversations.findConversationByTopic(group.topic) }
        assertEquals(group.id, sameGroup?.id)
        assertEquals(dm.id, sameDm?.id)
    }

    @Test
    fun testsCanListConversations() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 1)
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 1)

        runBlocking { caroClient.conversations.sync() }
        assertEquals(
            runBlocking { caroClient.conversations.list().size },
            2
        )
        assertEquals(runBlocking { caroClient.conversations.listGroups().size }, 1)
    }

    @Test
    fun testsCanListConversationsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(
            runBlocking { boClient.conversations.list(consentStates = listOf(ConsentState.ALLOWED)).size },
            2
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking { boClient.conversations.list(consentStates = listOf(ConsentState.ALLOWED)).size },
            1
        )
        assertEquals(
            runBlocking { boClient.conversations.list(consentStates = listOf(ConsentState.DENIED)).size },
            1
        )
        assertEquals(
            runBlocking {
                boClient.conversations.list(
                    consentStates = listOf(
                        ConsentState.DENIED,
                        ConsentState.ALLOWED
                    )
                ).size
            },
            2
        )
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
    }

    @Test
    fun testCanListConversationsOrder() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group1 =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val group2 =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val dmMessage = runBlocking { dm.send("Howdy") }
        val groupMessage = runBlocking { group2.send("Howdy") }
        runBlocking { boClient.conversations.syncAllConversations() }
        val conversations = runBlocking { boClient.conversations.list() }
        assertEquals(conversations.size, 3)
        assertEquals(conversations.map { it.id }, listOf(group2.id, dm.id, group1.id))
        runBlocking {
            assertEquals(group2.lastMessage()!!.id, groupMessage)
            assertEquals(dm.lastMessage()!!.id, dmMessage)
        }
    }

    @Test
    fun testsCanSyncAllConversationsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group =
            runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 2)
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(
                        ConsentState.ALLOWED
                    )
                )
            }.toInt() >= 2
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(
                        ConsentState.DENIED
                    )
                )
            }.toInt() <= 1
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(
                        ConsentState.ALLOWED
                    )
                )
            }.toInt() <= 2
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(
                        ConsentState.DENIED
                    )
                )
            }.toInt() <= 2
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(
                        ConsentState.DENIED,
                        ConsentState.ALLOWED
                    )
                )
            }.toInt() >= 2
        )
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 2)
    }

    @Test
    fun testCanStreamAllMessages() {
        val group =
            runBlocking { caroClient.conversations.newGroup(listOf(boClient.inboxId)) }
        val conversation =
            runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.sync() }

        val allMessages = mutableListOf<Message>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                boClient.conversations.streamAllMessages()
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)
        runBlocking {
            group.send("hi")
            conversation.send("hi")
        }
        Thread.sleep(1000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }

    @Test
    fun testCanStreamGroupsAndConversations() {
        val allMessages = mutableListOf<String>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                boClient.conversations.stream()
                    .collect { message ->
                        allMessages.add(message.topic)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)

        runBlocking {
            caroClient.conversations.newGroup(listOf(boClient.inboxId))
            Thread.sleep(1000)
            boClient.conversations.findOrCreateDm(caroClient.inboxId)
        }

        Thread.sleep(2000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }

    @Test
    fun testReturnsAllHMACKeys() {
        val conversations = mutableListOf<Conversation>()
        repeat(5) {
            val account = PrivateKeyBuilder()
            val client = runBlocking { Client.create(account, fixtures.clientOptions) }
            runBlocking {
                conversations.add(
                    alixClient.conversations.newConversation(client.inboxId)
                )
            }
        }
        val hmacKeys = alixClient.conversations.getHmacKeys()

        val topics = hmacKeys.hmacKeysMap.keys
        conversations.forEach { convo ->
            assertTrue(topics.contains(convo.topic))
        }
    }

    @Test
    fun testStreamsAndMessages() = runBlocking {
        val messages = mutableListOf<String>()
        val alixGroup =
            alixClient.conversations.newGroup(listOf(caroClient.inboxId, boClient.inboxId))
        val caroGroup2 =
            caroClient.conversations.newGroup(listOf(alixClient.inboxId, boClient.inboxId))

        alixClient.conversations.syncAllConversations()
        caroClient.conversations.syncAllConversations()
        boClient.conversations.syncAllConversations()

        val boGroup = boClient.conversations.findGroup(alixGroup.id)!!
        val caroGroup = caroClient.conversations.findGroup(alixGroup.id)!!
        val boGroup2 = boClient.conversations.findGroup(caroGroup2.id)!!
        val alixGroup2 = alixClient.conversations.findGroup(caroGroup2.id)!!

        val caroJob = launch(Dispatchers.IO) {
            println("Caro is listening...")
            try {
                withTimeout(60.seconds) { // Ensure test doesn't hang indefinitely
                    caroClient.conversations.streamAllMessages()
                        .take(100) // Stop after receiving 90 messages
                        .collect { message ->
                            synchronized(messages) { messages.add(message.body) }
                            println("Caro received: ${message.body}")
                        }
                }
            } catch (e: TimeoutCancellationException) {
                println("Timeout reached for caroJob")
            }
        }

        delay(1000)

        // Simulate message sending in multiple threads
        val alixJob = launch(Dispatchers.IO) {
            println("Alix is sending messages...")
            repeat(20) {
                val message = "Alix Message $it"
                alixGroup.send(message)
                alixGroup2.send(message)
                println("Alix sent: $message")
            }
        }

        val boMessageJob = launch(Dispatchers.IO) {
            println("Bo is sending messages..")
            repeat(10) {
                val message = "Bo Message $it"
                boGroup.send(message)
                boGroup2.send(message)
                println("Bo sent: $message")
            }
        }

        val davonSpamJob = launch(Dispatchers.IO) {
            println("Davon is sending spam groups..")
            repeat(10) {
                val spamMessage = "Davon Spam Message $it"
                val group = davonClient.conversations.newGroup(listOf(caroClient.inboxId))
                group.send(spamMessage)
                println("Davon spam: $spamMessage")
            }
        }

        val caroMessagingJob = launch(Dispatchers.IO) {
            println("Caro is sending messages...")
            repeat(10) {
                val message = "Caro Message $it"
                caroGroup.send(message)
                caroGroup2.send(message)
                println("Caro sent: $message")
            }
        }

        joinAll(alixJob, caroMessagingJob, boMessageJob, davonSpamJob)

        // Wait a bit to ensure all messages are processed
        delay(2000)

        caroJob.cancelAndJoin()

        assertEquals(90, messages.size)
        assertEquals(40, caroGroup.messages().size)

        boGroup.sync()
        alixGroup.sync()
        caroGroup.sync()

        assertEquals(40, boGroup.messages().size)
        assertEquals(41, alixGroup.messages().size)
        assertEquals(40, caroGroup.messages().size)
    }
}
