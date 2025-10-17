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
import org.xmtp.android.library.libxmtp.ConversationDebugInfo
import org.xmtp.android.library.libxmtp.DecodedMessage
import kotlin.time.Duration.Companion.seconds

@RunWith(AndroidJUnit4::class)
class ConversationsTest : BaseInstrumentedTest() {
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
    fun testCanCreateOptimisticGroup() =
        runBlocking {
            val optimisticGroup = boClient.conversations.newGroupOptimistic(groupName = "Testing")
            assertEquals(optimisticGroup.name(), "Testing")
            runBlocking { optimisticGroup.prepareMessage("testing") }
            assertEquals(optimisticGroup.messages().size, 1)

            optimisticGroup.addMembers(listOf(alixClient.inboxId))
            optimisticGroup.sync()
            optimisticGroup.publishMessages()
            assertEquals(optimisticGroup.messages().size, 2)
            assertEquals(optimisticGroup.members().size, 2)
            assertEquals(optimisticGroup.name(), "Testing")
        }

    @Test
    fun testsCanFindConversationByTopic() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
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
        assertEquals(runBlocking { caroClient.conversations.list().size }, 2)
        assertEquals(runBlocking { caroClient.conversations.listGroups().size }, 1)
    }

    @Test
    fun testsCanListConversationsAndCheckCommitLogForkStatus() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(runBlocking { boClient.conversations.listDms().size }, 1)
        assertEquals(runBlocking { boClient.conversations.listGroups().size }, 1)

        runBlocking { caroClient.conversations.sync() }
        val caroConversations = runBlocking { caroClient.conversations.list() }
        assertEquals(caroConversations.size, 2)
        var numForkStatusUnknown = 0
        var numForkStatusForked = 0
        var numForkStatusNotForked = 0
        for (conversation in caroConversations) {
            when (conversation.commitLogForkStatus()) {
                ConversationDebugInfo.CommitLogForkStatus.FORKED -> numForkStatusForked += 1
                ConversationDebugInfo.CommitLogForkStatus.NOT_FORKED -> numForkStatusNotForked += 1
                ConversationDebugInfo.CommitLogForkStatus.UNKNOWN -> numForkStatusUnknown += 1
            }
        }
        // Right now worker runs every 5 minutes so we'd need to wait that long to verify not forked
        assertEquals(numForkStatusForked, 0)
        assertEquals(numForkStatusNotForked, 0)
        assertEquals(numForkStatusUnknown, 2)
    }

    @Test
    fun testsCanListConversationsFiltered() {
        runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assertEquals(runBlocking { boClient.conversations.list().size }, 2)
        assertEquals(
            runBlocking {
                boClient.conversations.list(consentStates = listOf(ConsentState.ALLOWED)).size
            },
            2,
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assertEquals(
            runBlocking {
                boClient.conversations.list(consentStates = listOf(ConsentState.ALLOWED)).size
            },
            1,
        )
        assertEquals(
            runBlocking {
                boClient.conversations.list(consentStates = listOf(ConsentState.DENIED)).size
            },
            1,
        )
        assertEquals(
            runBlocking {
                boClient.conversations
                    .list(
                        consentStates =
                            listOf(ConsentState.DENIED, ConsentState.ALLOWED),
                    ).size
            },
            2,
        )
        assertEquals(runBlocking { boClient.conversations.list().size }, 1)
    }

    @Test
    fun testCanListConversationsOrder() {
        val dm = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val group1 = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val group2 = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
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
        val group = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 2)
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.ALLOWED),
                )
            }.toInt() >= 2,
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.DENIED),
                )
            }.toInt() <= 1,
        )
        runBlocking { group.updateConsentState(ConsentState.DENIED) }
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.ALLOWED),
                )
            }.toInt() <= 2,
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.DENIED),
                )
            }.toInt() <= 2,
        )
        assert(
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates =
                        listOf(ConsentState.DENIED, ConsentState.ALLOWED),
                )
            }.toInt() >= 2,
        )
        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 1)
    }

    @Test
    fun testCanStreamAllMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(boClient.inboxId)) }
        val conversation = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        runBlocking { boClient.conversations.sync() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    boClient.conversations.streamAllMessages().collect { message ->
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
    fun testCanStreamAllMessagesFilterConsent() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(caroClient.inboxId)) }
        val conversation = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        val blockedGroup =
            runBlocking {
                boClient.conversations.newGroup(listOf(alixClient.inboxId))
            }
        val blockedConversation =
            runBlocking {
                boClient.conversations.findOrCreateDm(alixClient.inboxId)
            }
        runBlocking {
            blockedGroup.updateConsentState(ConsentState.DENIED)
            blockedConversation.updateConsentState(ConsentState.DENIED)
            boClient.conversations.sync()
        }

        val allMessages = mutableListOf<DecodedMessage>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    boClient.conversations
                        .streamAllMessages(
                            consentStates = listOf(ConsentState.ALLOWED),
                        ).collect { message -> allMessages.add(message) }
                } catch (e: Exception) {
                }
            }
        Thread.sleep(1000)
        runBlocking {
            group.send("hi")
            conversation.send("hi")
            blockedGroup.send("hi")
            blockedConversation.send("hi")
        }
        Thread.sleep(1000)
        assertEquals(2, allMessages.size)
        job.cancel()
    }

    @Test
    fun testCanStreamGroupsAndConversations() {
        val allMessages = mutableListOf<String>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    boClient.conversations.stream().collect { message ->
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
            val account = createWallet()
            val client = runBlocking { createClient(account) }
            runBlocking {
                conversations.add(alixClient.conversations.newConversation(client.inboxId))
            }
        }
        val hmacKeys = runBlocking { alixClient.conversations.getHmacKeys() }

        val topics = hmacKeys.hmacKeysMap.keys
        conversations.forEach { convo -> assertTrue(topics.contains(convo.topic)) }
    }

    @Test
    fun testReturnsAllTopics() {
        val eriWallet = createWallet()
        val eriClient = runBlocking { createClient(eriWallet) }
        val dm1 = runBlocking { eriClient.conversations.newConversation(boClient.inboxId) }
        val group = runBlocking { boClient.conversations.newGroup(listOf(eriClient.inboxId)) }
        val eriClient2 = runBlocking { createClient(eriWallet) }
        val dm2 = runBlocking { eriClient2.conversations.newConversation(boClient.inboxId) }

        runBlocking {
            boClient.conversations.syncAllConversations()
            eriClient2.conversations.syncAllConversations()
            eriClient.conversations.syncAllConversations()
        }

        val allTopics = runBlocking { eriClient.conversations.allPushTopics() }
        val conversations = runBlocking { eriClient.conversations.list() }
        val allHmacKeys = runBlocking { eriClient.conversations.getHmacKeys() }
        val dmHmacKeys = runBlocking { dm1.getHmacKeys() }
        val dmTopics = runBlocking { dm1.getPushTopics() }

        assertEquals(allTopics.size, 3)
        assertEquals(conversations.size, 2)

        val hmacTopics = allHmacKeys.hmacKeysMap.keys
        allTopics.forEach { topic -> assertTrue(hmacTopics.contains(topic)) }

        assertEquals(dmTopics.size, 2)
        assertTrue(allTopics.containsAll(dmTopics))

        val dmHmacTopics = dmHmacKeys.hmacKeysMap.keys
        dmTopics.forEach { topic -> assertTrue(dmHmacTopics.contains(topic)) }
    }

    @Test
    fun testPaginationOfConversationsList() =
        runBlocking {
            // Create 15 groups
            val groups = mutableListOf<Group>()
            for (i in 0..14) {
                val group =
                    boClient.conversations.newGroup(
                        listOf(caroClient.inboxId),
                        groupName = "Test Group $i",
                    )
                groups.add(group)
            }

            // Send a message to half the groups to ensure they're ordered by last message
            // and not by created_at
            groups.forEachIndexed { index, group ->
                if (index % 2 == 0) {
                    group.send("Sending a message to ensure filtering by last message time works")
                }
            }

            // Track all conversations retrieved through pagination
            val allConversations = mutableSetOf<String>()
            var pageCount = 0
            // Get the first page
            var page =
                boClient.conversations.listGroups(
                    limit = 5,
                )

            while (page.isNotEmpty()) {
                pageCount++
                // Add new conversation IDs to our set
                page.forEach { conversation ->
                    if (allConversations.contains(conversation.id)) {
                        throw AssertionError("Duplicate conversation ID found: ${conversation.id}")
                    }
                    allConversations.add(conversation.id)
                }

                // If we got fewer than the limit, we've reached the end
                if (page.size < 5) {
                    break
                }

                // Get the oldest (last) conversation's timestamp for the next page
                val lastConversation = page.last()

                // Get the next page - subtract 1 nanosecond to avoid including the same conversation
                page =
                    boClient.conversations.listGroups(
                        lastActivityBeforeNs = lastConversation.lastActivityNs,
                        limit = 5,
                    )

                // Safety check to prevent infinite loop
                if (pageCount > 10) {
                    throw AssertionError("Too many pages, possible infinite loop")
                }
            }

            // Validate results
            assertEquals("Should have retrieved all 15 groups", 15, allConversations.size)

            // Verify all created groups are in the results
            groups.forEach { group ->
                assertTrue(
                    "Group ${group.id} should be in paginated results",
                    allConversations.contains(group.id),
                )
            }
        }

    @Test
    fun testStreamsAndMessages() =
        runBlocking {
            val messages = mutableListOf<String>()
            val davonClient = createClient(createWallet())
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

            val caroJob =
                launch(Dispatchers.IO) {
                    println("Caro is listening...")
                    try {
                        withTimeout(60.seconds) {
                            // Ensure test doesn't hang indefinitely
                            caroClient
                                .conversations
                                .streamAllMessages()
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
            val alixJob =
                launch(Dispatchers.IO) {
                    println("Alix is sending messages...")
                    repeat(20) {
                        val message = "Alix Message $it"
                        alixGroup.send(message)
                        alixGroup2.send(message)
                        println("Alix sent: $message")
                    }
                }

            val boMessageJob =
                launch(Dispatchers.IO) {
                    println("Bo is sending messages..")
                    repeat(10) {
                        val message = "Bo Message $it"
                        boGroup.send(message)
                        boGroup2.send(message)
                        println("Bo sent: $message")
                    }
                }

            val davonSpamJob =
                launch(Dispatchers.IO) {
                    println("Davon is sending spam groups..")
                    repeat(10) {
                        val spamMessage = "Davon Spam Message $it"
                        val group = davonClient.conversations.newGroup(listOf(caroClient.inboxId))
                        group.send(spamMessage)
                        println("Davon spam: $spamMessage")
                    }
                }

            val caroMessagingJob =
                launch(Dispatchers.IO) {
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
            assertEquals(41, caroGroup.messages().size)

            boGroup.sync()
            alixGroup.sync()
            caroGroup.sync()

            assertEquals(41, boGroup.messages().size)
            assertEquals(41, alixGroup.messages().size)
            assertEquals(41, caroGroup.messages().size)
        }
}
