package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.ConversationDebugInfo
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.messages.PrivateKeyBuilder
import uniffi.xmtpv3.FfiConversationMessageKind
import java.security.SecureRandom

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
        val syncSummary = runBlocking { boClient.conversations.syncAllConversations() }
        assert(syncSummary.numEligible >= 2U)
        var syncSummaryAllowed =
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.ALLOWED),
                )
            }

        assert(syncSummaryAllowed.numEligible >= 2U)

        var syncSummaryDenied =
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.DENIED),
                )
            }
        assert(syncSummaryDenied.numEligible <= 1U)
        runBlocking { group.updateConsentState(ConsentState.DENIED) }

        syncSummaryAllowed =
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.ALLOWED),
                )
            }
        assert(syncSummaryAllowed.numEligible <= 2U)
        syncSummaryDenied =
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.DENIED),
                )
            }
        assert(syncSummaryDenied.numEligible <= 2U)

        var syncSummaryAllowedDenied =
            runBlocking {
                boClient.conversations.syncAllConversations(
                    consentStates = listOf(ConsentState.ALLOWED, ConsentState.DENIED),
                )
            }
        assert(syncSummaryAllowedDenied.numEligible >= 2U)
//        assert(runBlocking { boClient.conversations.syncAllConversations() }.toInt() >= 1)
    }

    @Test
    fun testCanStreamAllMessages() {
        val group = runBlocking { caroClient.conversations.newGroup(listOf(boClient.inboxId)) }
        val conversation = runBlocking { boClient.conversations.findOrCreateDm(caroClient.inboxId) }
        // Sync to drain any membership change messages before starting the stream
        runBlocking { boClient.conversations.syncAllConversations() }

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
        val applicationMessages =
            allMessages.filter {
                it.kind == FfiConversationMessageKind.APPLICATION
            }
        assertEquals(2, applicationMessages.size)
        job.cancel()
    }

    @Test
    fun testCanStreamAllMessagesFilterConsent() =
        runBlocking {
            val group = boClient.conversations.newGroup(listOf(caroClient.inboxId))
            val conversation = boClient.conversations.findOrCreateDm(caroClient.inboxId)
            val blockedGroup = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            val blockedConversation = boClient.conversations.findOrCreateDm(alixClient.inboxId)
            // Sending sets consent to ALLOWED. Deny these conversations after the retained sends.
            val blockedIds = setOf(blockedGroup.send("blocked group"), blockedConversation.send("blocked dm"))
            blockedGroup.updateConsentState(ConsentState.DENIED)
            blockedConversation.updateConsentState(ConsentState.DENIED)
            boClient.conversations.sync()
            alixClient.conversations.syncAllConversations()
            val peerBlockedGroup = requireNotNull(alixClient.conversations.findGroup(blockedGroup.id))
            val peerBlockedDm = requireNotNull(alixClient.conversations.findDmByInboxId(boClient.inboxId))

            val messages = StreamTestMessages()
            val job =
                launch(Dispatchers.IO) {
                    boClient.conversations
                        .streamAllMessages(consentStates = listOf(ConsentState.ALLOWED))
                        .collect { messages.add(it) }
                }
            try {
                val retained =
                    boClient.conversations
                        .messageHistorySnapshot(
                            10U,
                            consentStates = listOf(ConsentState.ALLOWED),
                        ).messages
                assertEquals(2, retained.size)
                assertTrue(retained.all { it.kind == FfiConversationMessageKind.MEMBERSHIP_CHANGE })
                messages.awaitHistory(retained)

                val expected = mutableListOf(group.send("group hi") to "group hi")
                messages.awaitApplications(expected)
                expected.add(conversation.send("dm hi") to "dm hi")
                messages.awaitApplications(expected)
                val blockedLiveIds =
                    setOf(peerBlockedGroup.send("blocked live group"), peerBlockedDm.send("blocked live dm"))
                blockedGroup.sync()
                blockedConversation.sync()
                val storedBlockedIds =
                    (blockedGroup.messages() + blockedConversation.messages()).map { it.id }.toSet()
                assertTrue(storedBlockedIds.containsAll(blockedIds + blockedLiveIds))

                // Keep a full exclusion window after allowed delivery completes.
                delay(1000)
                val history =
                    boClient.conversations
                        .messageHistorySnapshot(
                            10U,
                            consentStates = listOf(ConsentState.ALLOWED),
                        ).messages
                assertEquals(4, history.size)
                assertEquals(setOf(group.id, conversation.id), history.map { it.conversationId }.toSet())
                assertTrue(messages.snapshot().none { it.id in blockedIds || it.id in blockedLiveIds })
                assertEquals(ConsentState.DENIED, blockedGroup.consentState())
                assertEquals(ConsentState.DENIED, blockedConversation.consentState())
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
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
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val eriWallet = PrivateKeyBuilder()

        val eriClient =
            runBlocking {
                Client.create(
                    account = eriWallet,
                    options =
                        ClientOptions(
                            localApi(),
                            appContext = context,
                            dbEncryptionKey = key,
                        ),
                )
            }
        val dm1 = runBlocking { eriClient.conversations.newConversation(boClient.inboxId) }
        runBlocking { boClient.conversations.newGroup(listOf(eriClient.inboxId)) }
        val eriClient2 =
            runBlocking {
                Client.create(
                    account = eriWallet,
                    options =
                        ClientOptions(
                            localApi(),
                            appContext = context,
                            dbEncryptionKey = key,
                            dbDirectory = context.filesDir.absolutePath.toString(),
                        ),
                )
            }
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
            val messages = StreamTestMessages()
            val expectedApplications = mutableListOf<Triple<String, String, String>>()
            val expectedGroups = mutableSetOf<String>()

            suspend fun sendExpected(
                group: Group,
                body: String,
            ) {
                val id = group.send(body)
                synchronized(expectedApplications) {
                    expectedApplications.add(Triple(id, group.id, body))
                }
            }
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
            expectedGroups.addAll(listOf(alixGroup.id, caroGroup2.id))

            val caroJob =
                launch(Dispatchers.IO) {
                    caroClient.conversations.streamAllMessages().collect { messages.add(it) }
                }

            try {
                val retained = caroClient.conversations.messageHistorySnapshot(200U).messages
                assertEquals(2, retained.size)
                assertTrue(retained.all { it.kind == FfiConversationMessageKind.MEMBERSHIP_CHANGE })
                messages.awaitHistory(retained)

                // Simulate message sending in multiple threads
                val alixJob =
                    launch(Dispatchers.IO) {
                        println("Alix is sending messages...")
                        repeat(20) {
                            val message = "Alix Message $it"
                            sendExpected(alixGroup, message)
                            sendExpected(alixGroup2, message)
                            println("Alix sent: $message")
                            // 50ms yield between sends so a single sender's MLS
                            // ratchet doesn't outrun OpenMLS's 5-generation
                            // out-of-order tolerance under concurrent load (#3512).
                            delay(50)
                        }
                    }

                val boMessageJob =
                    launch(Dispatchers.IO) {
                        println("Bo is sending messages..")
                        repeat(10) {
                            val message = "Bo Message $it"
                            sendExpected(boGroup, message)
                            sendExpected(boGroup2, message)
                            println("Bo sent: $message")
                            delay(50) // #3512
                        }
                    }

                val davonSpamJob =
                    launch(Dispatchers.IO) {
                        println("Davon is sending spam groups..")
                        repeat(10) {
                            val spamMessage = "Davon Spam Message $it"
                            val group = davonClient.conversations.newGroup(listOf(caroClient.inboxId))
                            synchronized(expectedGroups) { expectedGroups.add(group.id) }
                            sendExpected(group, spamMessage)
                            println("Davon spam: $spamMessage")
                            delay(50) // #3512
                        }
                    }

                val caroMessagingJob =
                    launch(Dispatchers.IO) {
                        println("Caro is sending messages...")
                        repeat(10) {
                            val message = "Caro Message $it"
                            sendExpected(caroGroup, message)
                            sendExpected(caroGroup2, message)
                            println("Caro sent: $message")
                            delay(50) // #3512
                        }
                    }

                joinAll(alixJob, caroMessagingJob, boMessageJob, davonSpamJob)

                withTimeout(60_000) {
                    while (messages.snapshot().count { it.kind == FfiConversationMessageKind.APPLICATION } < 90) {
                        delay(10)
                    }
                }

                val applications = messages.snapshot().filter { it.kind == FfiConversationMessageKind.APPLICATION }
                assertEquals(90, expectedApplications.size)
                assertEquals(
                    expectedApplications.associate { it.first to (it.second to it.third) },
                    applications.associate { it.id to (it.conversationId to it.body) },
                )
                // Each sender keeps its order within each group. Different senders can interleave.
                expectedApplications
                    .groupBy { it.second to it.third.substringBefore(" Message") }
                    .forEach { (source, sent) ->
                        assertEquals(
                            sent.map { it.first },
                            applications
                                .filter {
                                    it.conversationId == source.first &&
                                        it.body.substringBefore(" Message") == source.second
                                }.map { it.id },
                        )
                    }
                val history = caroClient.conversations.messageHistorySnapshot(200U).messages
                assertEquals(102, history.size)
                val memberships = history.filter { it.kind == FfiConversationMessageKind.MEMBERSHIP_CHANGE }
                assertEquals(12, memberships.size)
                assertEquals(expectedGroups, memberships.map { it.conversationId }.toSet())
                messages.awaitHistory(history)
                assertEquals(41, caroGroup.messages().size)

                boGroup.sync()
                alixGroup.sync()
                caroGroup.sync()

                assertEquals(41, boGroup.messages().size)
                assertEquals(41, alixGroup.messages().size)
                assertEquals(41, caroGroup.messages().size)
            } finally {
                withContext(NonCancellable) { caroJob.cancelAndJoin() }
            }
        }

    @Test
    fun testDeleteMessage() =
        runBlocking {
            val group = caroClient.conversations.newGroup(listOf(boClient.inboxId))

            val messageID = group.send("Hi there")

            val originalNumberOfMessages = group.messages().size

            caroClient.conversations.deleteMessageLocally(messageID)

            assertEquals(originalNumberOfMessages - 1, group.messages().size)
        }

    @Test
    fun testCountMessages() =
        runBlocking {
            // Test with Group conversation
            val group = boClient.conversations.newGroup(listOf(alixClient.inboxId))

            // Send some messages
            group.send("Message 1")
            group.send("Message 2")
            group.send("Message 3")

            // Count all messages
            val groupCount = group.countMessages()
            assertEquals(4L, groupCount) // 3 messages + 1 member added message

            // Test with DM conversation
            val dm = boClient.conversations.findOrCreateDm(caroClient.inboxId)

            // Send some messages
            dm.send("DM Message 1")
            val msg2ID = dm.send("DM Message 2")
            val msg2 = boClient.conversations.findMessage(msg2ID)

            val dmCount = dm.countMessages()
            assertEquals(2L, dmCount)

            // Test with Conversation wrapper
            val conversation = Conversation.Dm(dm)
            val conversationCount = conversation.countMessages()
            assertEquals(2L, conversationCount)

            val msg3ID = dm.send("DM Message 3")
            val msg3 = boClient.conversations.findMessage(msg3ID)

            val countBefore = dm.countMessages(beforeNs = msg3!!.sentAtNs)
            assertEquals(2L, countBefore)

            val countAfter = dm.countMessages(afterNs = msg2!!.sentAtNs)
            assertEquals(1L, countAfter)

            // Test with delivery status filtering
            val unpublishedId = dm.prepareMessage("Unpublished message")

            val allCount = dm.countMessages(deliveryStatus = DecodedMessage.MessageDeliveryStatus.ALL)
            val publishedCount =
                dm.countMessages(deliveryStatus = DecodedMessage.MessageDeliveryStatus.PUBLISHED)
            val unpublishedCount =
                dm.countMessages(deliveryStatus = DecodedMessage.MessageDeliveryStatus.UNPUBLISHED)

            assertEquals(4L, allCount)
            assertEquals(3L, publishedCount)
            assertEquals(1L, unpublishedCount)

            // Publish the message and verify counts
            dm.publishMessages()

            val publishedCountAfter =
                dm.countMessages(deliveryStatus = DecodedMessage.MessageDeliveryStatus.PUBLISHED)
            val unpublishedCountAfter =
                dm.countMessages(deliveryStatus = DecodedMessage.MessageDeliveryStatus.UNPUBLISHED)

            assertEquals(4L, publishedCountAfter)
            assertEquals(0L, unpublishedCountAfter)
        }

    @Test
    fun testCanStreamMessageDeletions() {
        val deletedMessageIds = mutableListOf<String>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    boClient.conversations.streamMessageDeletions().collect { message ->
                        deletedMessageIds.add(message.id)
                    }
                } catch (e: Exception) {
                }
            }

        Thread.sleep(1000)

        runBlocking {
            val disappearingSettings =
                DisappearingMessageSettings(
                    1_000_000_000,
                    1_000_000_000,
                )

            val dm =
                boClient.conversations.findOrCreateDm(
                    alixClient.inboxId,
                    disappearingMessageSettings = disappearingSettings,
                )

            val messageId = dm.send("This message will disappear")

            Thread.sleep(3000)

            assertTrue(deletedMessageIds.contains(messageId))
        }

        job.cancel()
    }
}
