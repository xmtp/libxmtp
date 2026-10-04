package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class ConversationsTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private fun id(value: Conversation): String =
        when (value) {
            is Conversation.Group -> value.group.id()
            is Conversation.Dm -> value.dm.id()
        }

    private fun topic(value: Conversation): String =
        when (value) {
            is Conversation.Group -> value.group.topic()
            is Conversation.Dm -> value.dm.topic()
        }

    private suspend fun state(value: Conversation): ConversationState =
        when (value) {
            is Conversation.Group -> value.group.state().common
            is Conversation.Dm -> value.dm.state()
        }

    private suspend fun findGroup(
        client: SDKClient,
        id: String,
    ): Group = client.conversations().listGroups(null).single { it.id() == id }

    @Test fun testCanCreateOptimisticGroup() =
        runBlocking {
            val group = bo.conversations().createGroupOptimistic(CreateGroupOptions(name = "Testing"))
            assertEquals("Testing", group.state().name)
            group.prepareMessage(TextCodec().encode("testing"))
            assertEquals(1, group.messages().size)
            group.addMembers(listOf(alix.inboxId()))
            group.sync()
            group.publishMessages()
            assertEquals(2, group.messages().size)
            assertEquals(2, group.members().size)
            assertEquals("Testing", group.state().name)
        }

    @Test fun testsCanFindConversationByTopic() =
        runBlocking {
            val group = bo.conversations().createGroup(listOf(caro.inboxId()))
            val dm = bo.conversations().createDm(caro.inboxId())
            val listed = bo.conversations().list()
            assertEquals(group.id(), id(listed.single { topic(it) == group.topic() }))
            assertEquals(dm.id(), id(listed.single { topic(it) == dm.topic() }))
        }

    @Test fun testsCanListConversations() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            bo.conversations().createGroup(listOf(caro.inboxId()))
            assertEquals(2, bo.conversations().list().size)
            assertEquals(1, bo.conversations().listDms(null).size)
            assertEquals(1, bo.conversations().listGroups(null).size)
            caro.conversations().sync()
            assertEquals(2, caro.conversations().list().size)
            assertEquals(1, caro.conversations().listGroups(null).size)
        }

    @Test fun testsCanListConversationsAndCheckCommitLogForkStatus() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            bo.conversations().createGroup(listOf(caro.inboxId()))
            assertEquals(2, bo.conversations().list().size)
            assertEquals(1, bo.conversations().listDms(null).size)
            assertEquals(1, bo.conversations().listGroups(null).size)
            caro.conversations().sync()
            val listed = caro.conversations().list()
            assertEquals(2, listed.size)
            assertEquals(
                listOf(CommitLogForkStatus.UNKNOWN, CommitLogForkStatus.UNKNOWN),
                listed.map { state(it).commitLogForkStatus },
            )
        }

    @Test fun testsCanListConversationsFiltered() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            val group = bo.conversations().createGroup(listOf(caro.inboxId()))
            val allowed = ListConversationsOptions(consentStates = listOf(ConsentState.ALLOWED))
            val denied = ListConversationsOptions(consentStates = listOf(ConsentState.DENIED))
            assertEquals(2, bo.conversations().list().size)
            assertEquals(2, bo.conversations().list(allowed).size)
            group.updateConsentState(ConsentState.DENIED)
            assertEquals(1, bo.conversations().list(allowed).size)
            assertEquals(1, bo.conversations().list(denied).size)
            assertEquals(
                2,
                bo
                    .conversations()
                    .list(
                        ListConversationsOptions(consentStates = listOf(ConsentState.ALLOWED, ConsentState.DENIED)),
                    ).size,
            )
            assertEquals(1, bo.conversations().list().size)
        }

    @Test fun testCanListConversationsOrder() =
        runBlocking {
            val dm = bo.conversations().createDm(caro.inboxId())
            val group1 = bo.conversations().createGroup(listOf(caro.inboxId()))
            val group2 = bo.conversations().createGroup(listOf(caro.inboxId()))
            val dmMessage = dm.sendText("Howdy")
            val groupMessage = group2.sendText("Howdy")
            bo.conversations().syncAll(null)
            assertEquals(listOf(group2.id(), dm.id(), group1.id()), bo.conversations().list().map(::id))
            assertEquals(groupMessage, group2.lastMessage()?.id)
            assertEquals(dmMessage, dm.lastMessage()?.id)
        }

    @Test fun testsCanSyncAllConversationsFiltered() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            val group = bo.conversations().createGroup(listOf(caro.inboxId()))
            assertTrue(bo.conversations().syncAll(null).eligible >= 2uL)
            assertTrue(bo.conversations().syncAll(listOf(ConsentState.ALLOWED)).eligible >= 2uL)
            assertTrue(bo.conversations().syncAll(listOf(ConsentState.DENIED)).eligible <= 1uL)
            group.updateConsentState(ConsentState.DENIED)
            assertTrue(bo.conversations().syncAll(listOf(ConsentState.ALLOWED)).eligible <= 2uL)
            assertTrue(bo.conversations().syncAll(listOf(ConsentState.DENIED)).eligible <= 2uL)
            assertTrue(bo.conversations().syncAll(listOf(ConsentState.ALLOWED, ConsentState.DENIED)).eligible >= 2uL)
        }

    @Test fun testCanStreamAllMessages() =
        runBlocking {
            val group = caro.conversations().createGroup(listOf(bo.inboxId()))
            val dm = bo.conversations().createDm(caro.inboxId())
            bo.conversations().syncAll(null)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages().collect { messages.add(it) } }
            try {
                messages.awaitHistory(bo.conversations().messageHistorySnapshot(10u).messages)
                val expected = listOf(group.sendText("hi") to "hi", dm.sendText("hi") to "hi")
                messages.awaitApplicationsAcrossConversations(expected) {
                    bo.conversations().messageHistorySnapshot(10u).messages
                }
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamAllMessagesFilterConsent() =
        runBlocking {
            val group = bo.conversations().createGroup(listOf(caro.inboxId()))
            val dm = bo.conversations().createDm(caro.inboxId())
            val blockedGroup = bo.conversations().createGroup(listOf(alix.inboxId()))
            val blockedDm = bo.conversations().createDm(alix.inboxId())
            val blockedIds = setOf(blockedGroup.sendText("blocked group"), blockedDm.sendText("blocked dm"))
            blockedGroup.updateConsentState(ConsentState.DENIED)
            blockedDm.updateConsentState(ConsentState.DENIED)
            bo.conversations().sync()
            alix.conversations().syncAll(null)
            val peerGroup = findGroup(alix, blockedGroup.id())
            val peerDm = checkNotNull(alix.conversations().getDmByInboxId(bo.inboxId()))
            val options = MessageReaderOptions(consentStates = listOf(ConsentState.ALLOWED))
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages(options).collect { messages.add(it) } }
            try {
                val retained = bo.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(2, retained.size)
                assertTrue(retained.all { it.kind == MessageKind.MEMBERSHIP_CHANGE })
                messages.awaitHistory(retained)
                val expected = mutableListOf(group.sendText("group hi") to "group hi")
                messages.awaitApplications(expected)
                expected.add(dm.sendText("dm hi") to "dm hi")
                messages.awaitApplications(expected)
                val blockedLive = setOf(peerGroup.sendText("blocked live group"), peerDm.sendText("blocked live dm"))
                blockedGroup.sync()
                blockedDm.sync()
                assertTrue(
                    (blockedGroup.messages() + blockedDm.messages())
                        .map { it.id }
                        .toSet()
                        .containsAll(blockedIds + blockedLive),
                )
                delay(1000)
                val history = bo.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(4, history.size)
                assertEquals(setOf(group.id(), dm.id()), history.map { it.conversationId }.toSet())
                assertTrue(messages.snapshot().none { it.id in blockedIds || it.id in blockedLive })
                assertEquals(ConsentState.DENIED, blockedGroup.state().common.consentState)
                assertEquals(ConsentState.DENIED, blockedDm.state().consentState)
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamGroupsAndConversations() =
        runBlocking {
            val received = mutableListOf<Conversation>()
            val job =
                launch(Dispatchers.IO) {
                    bo.conversationStream().collect { synchronized(received) { received.add(it) } }
                }
            try {
                val group = caro.conversations().createGroup(listOf(bo.inboxId()))
                val dm = bo.conversations().createDm(caro.inboxId())
                withTimeout(10_000) { while (synchronized(received) { received.size } < 2) delay(10) }
                val snapshot = synchronized(received) { received.toList() }
                assertEquals(2, snapshot.size)
                assertEquals(setOf(group.id(), dm.id()), snapshot.map(::id).toSet())
                assertEquals(setOf(group.topic(), dm.topic()), snapshot.map(::topic).toSet())
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testReturnsAllHMACKeys() =
        runBlocking {
            val dms = List(5) { alix.conversations().createDm(createClient(createWallet()).inboxId()) }
            val keys = alix.conversations().hmacKeys()
            assertTrue(keys.keys.containsAll(dms.map { it.id() }))
        }

    @Test fun testHmacKeysIncludeDuplicateDms() =
        runBlocking {
            val account = createWallet()
            val first = createClient(account)
            val dm1 = first.conversations().createDm(bo.inboxId())
            bo.conversations().createGroup(listOf(first.inboxId()))
            val second = createClient(account)
            second.conversations().createDm(bo.inboxId())
            bo.conversations().syncAll(null)
            second.conversations().syncAll(null)
            first.conversations().syncAll(null)
            val listed = first.conversations().list()
            val allKeys = first.conversations().hmacKeys()
            val dmKeys = dm1.hmacKeys()
            assertEquals(3, allKeys.size)
            assertEquals(2, listed.size)
            assertEquals(3, dmKeys.size)
            assertEquals(listOf(dmKeys[1].epoch - 1, dmKeys[1].epoch, dmKeys[1].epoch + 1), dmKeys.map { it.epoch })
            assertTrue(allKeys.values.flatten().containsAll(dmKeys))
        }

    @Test fun testPaginationOfConversationsList() =
        runBlocking {
            val groups =
                List(15) { index ->
                    bo.conversations().createGroup(
                        listOf(caro.inboxId()),
                        CreateGroupOptions(name = "Test Group $index"),
                    )
                }
            groups.forEachIndexed { index, group -> if (index % 2 == 0) group.sendText("activity") }
            val ids = mutableSetOf<String>()
            var pages = 0
            var page = bo.conversations().listGroups(ListConversationsOptions(limit = 5u))
            while (page.isNotEmpty()) {
                pages++
                page.forEach { assertTrue("duplicate conversation", ids.add(it.id())) }
                if (page.size < 5) break
                page =
                    bo.conversations().listGroups(
                        ListConversationsOptions(lastActivityBefore = page.last().lastActivityAt(null), limit = 5u),
                    )
                assertTrue("too many pages", pages <= 10)
            }
            assertEquals(15, ids.size)
            assertEquals(groups.map { it.id() }.toSet(), ids)
        }

    @Test fun testStreamsAndMessages() =
        runBlocking {
            val messages = StreamTestMessages()
            val expected = mutableListOf<Triple<String, String, String>>()
            val expectedGroups = mutableSetOf<String>()

            suspend fun send(
                group: Group,
                body: String,
            ) {
                val messageId = group.sendText(body)
                synchronized(expected) { expected.add(Triple(messageId, group.id(), body)) }
            }
            val davon = createClient(createWallet())
            val alixGroup = alix.conversations().createGroup(listOf(caro.inboxId(), bo.inboxId()))
            val caroGroup2 = caro.conversations().createGroup(listOf(alix.inboxId(), bo.inboxId()))
            listOf(alix, bo, caro).forEach { it.conversations().syncAll(null) }
            val boGroup = findGroup(bo, alixGroup.id())
            val caroGroup = findGroup(caro, alixGroup.id())
            val boGroup2 = findGroup(bo, caroGroup2.id())
            val alixGroup2 = findGroup(alix, caroGroup2.id())
            expectedGroups.addAll(listOf(alixGroup.id(), caroGroup2.id()))
            val reader = launch(Dispatchers.IO) { caro.messages().collect { messages.add(it) } }
            try {
                val retained = caro.conversations().messageHistorySnapshot(200u).messages
                assertEquals(2, retained.size)
                assertTrue(retained.all { it.kind == MessageKind.MEMBERSHIP_CHANGE })
                messages.awaitHistory(retained)
                val senders =
                    listOf(
                        launch(Dispatchers.IO) {
                            repeat(20) {
                                send(alixGroup, "Alix Message $it")
                                send(alixGroup2, "Alix Message $it")
                                delay(50)
                            }
                        },
                        launch(Dispatchers.IO) {
                            repeat(10) {
                                send(boGroup, "Bo Message $it")
                                send(boGroup2, "Bo Message $it")
                                delay(50)
                            }
                        },
                        launch(Dispatchers.IO) {
                            repeat(10) {
                                val spam = davon.conversations().createGroup(listOf(caro.inboxId()))
                                synchronized(expectedGroups) { expectedGroups.add(spam.id()) }
                                send(spam, "Davon Spam Message $it")
                                delay(50)
                            }
                        },
                        launch(Dispatchers.IO) {
                            repeat(10) {
                                send(caroGroup, "Caro Message $it")
                                send(caroGroup2, "Caro Message $it")
                                delay(50)
                            }
                        },
                    )
                senders.joinAll()
                withTimeout(60_000) {
                    while (messages.snapshot().count { it.kind == MessageKind.APPLICATION } <
                        90
                    ) {
                        delay(10)
                    }
                }
                val applications = messages.snapshot().filter { it.kind == MessageKind.APPLICATION }
                assertEquals(90, expected.size)
                assertEquals(
                    expected.associate { it.first to (it.second to it.third) },
                    applications.associate { it.id to (it.conversationId to messageText(it)) },
                )
                expected.groupBy { it.second to it.third.substringBefore(" Message") }.forEach { (source, sent) ->
                    assertEquals(
                        sent.map { it.first },
                        applications
                            .filter {
                                it.conversationId == source.first &&
                                    messageText(it).substringBefore(" Message") == source.second
                            }.map { it.id },
                    )
                }
                val history = caro.conversations().messageHistorySnapshot(200u).messages
                assertEquals(102, history.size)
                val memberships = history.filter { it.kind == MessageKind.MEMBERSHIP_CHANGE }
                assertEquals(12, memberships.size)
                assertEquals(expectedGroups, memberships.map { it.conversationId }.toSet())
                messages.awaitHistory(history)
                assertEquals(41, caroGroup.messages().size)
                listOf(boGroup, alixGroup, caroGroup).forEach {
                    it.sync()
                    assertEquals(41, it.messages().size)
                }
            } finally {
                withContext(NonCancellable) { reader.cancelAndJoin() }
            }
        }

    @Test fun testDeleteMessage() =
        runBlocking {
            val group = caro.conversations().createGroup(listOf(bo.inboxId()))
            val id = group.sendText("Hi there")
            val count = group.messages().size
            caro.conversations().deleteMessageLocally(id)
            assertEquals(count - 1, group.messages().size)
        }

    @Test fun testCountMessages() =
        runBlocking {
            val group = bo.conversations().createGroup(listOf(alix.inboxId()))
            repeat(3) { group.sendText("Message $it") }
            assertEquals(4uL, group.countMessages(null))
            val dm = bo.conversations().createDm(caro.inboxId())
            dm.sendText("DM Message 1")
            val second = dm.sendText("DM Message 2")
            val secondMessage = checkNotNull(bo.conversations().getMessageById(second))
            assertEquals(2uL, dm.countMessages(null))
            val wrapped = Conversation.Dm(dm)
            assertEquals(2uL, wrapped.dm.countMessages(null))
            val third = dm.sendText("DM Message 3")
            val thirdMessage = checkNotNull(bo.conversations().getMessageById(third))
            assertEquals(2uL, dm.countMessages(ListMessagesOptions(sentBefore = thirdMessage.sentAt)))
            assertEquals(1uL, dm.countMessages(ListMessagesOptions(sentAfter = secondMessage.sentAt)))
            dm.prepareMessage(TextCodec().encode("Unpublished message"))
            assertEquals(4uL, dm.countMessages(ListMessagesOptions(deliveryStatus = null)))
            assertEquals(3uL, dm.countMessages(ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)))
            assertEquals(1uL, dm.countMessages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)))
            dm.publishMessages()
            assertEquals(4uL, dm.countMessages(ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)))
            assertEquals(0uL, dm.countMessages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)))
        }

    @Test fun testCanStreamMessageDeletions() =
        runBlocking {
            val deleted = mutableSetOf<String>()
            val listener =
                bo.startListener(
                    EventFilter(listOf(EventKind.MESSAGE_EXPIRED), null, null, false),
                    { event ->
                        if (event is ClientEvent.MessageExpired) {
                            synchronized(
                                deleted,
                            ) { deleted.add(event.messageExpired.messageId) }
                        }
                    },
                )
            try {
                val dm =
                    bo.conversations().createDm(
                        alix.inboxId(),
                        CreateDmOptions(DisappearingSettings(Timestamp(1_000_000_000), 1_000_000_000)),
                    )
                val id = dm.sendText("This message will disappear")
                withTimeout(10_000) { while (!synchronized(deleted) { id in deleted }) delay(10) }
                assertTrue(synchronized(deleted) { id in deleted })
                assertNull(bo.conversations().getMessageById(id))
            } finally {
                withContext(NonCancellable) { bo.stopListener(listener) }
            }
        }
}
