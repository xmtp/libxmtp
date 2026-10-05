package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class DmTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private suspend fun find(
        client: SDKClient,
        peer: InboxId,
    ): Dm = checkNotNull(client.conversations().getDmByInboxId(peer))

    @Test fun testCanCreateADm() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            alix.conversations().sync()
            val same = alix.conversations().createDm(bo.inboxId())
            assertEquals(dm.id(), same.id())
            assertEquals(1uL, dm.debugInfo().epoch)
            assertFalse(dm.debugInfo().maybeForked)
            assertEquals("", dm.debugInfo().forkDetails)
        }

    @Test fun testCanSuccessfullyThreadDms() =
        runBlocking {
            val boDm = bo.conversations().createDm(alix.inboxId())
            val alixDm = alix.conversations().createDm(bo.inboxId())

            data class ExpectedDm(
                val creator: InboxId,
                val peer: InboxId,
            )

            data class ExpectedApplication(
                val sender: InboxId,
                val body: String,
                val group: ConversationId,
            )
            val physical = linkedMapOf(boDm.id() to ExpectedDm(bo.inboxId(), alix.inboxId()))
            physical.putIfAbsent(alixDm.id(), ExpectedDm(alix.inboxId(), bo.inboxId()))
            val applications = mutableMapOf<MessageId, ExpectedApplication>()

            fun assertHistory(
                messages: List<Message>,
                required: Set<ConversationId>,
            ) {
                assertEquals("Message IDs are unique", messages.size, messages.map { it.id }.toSet().size)
                val application = messages.filter { it.kind == MessageKind.APPLICATION }
                assertEquals(applications.keys, application.map { it.id }.toSet())
                val membership = messages.filter { it.kind == MessageKind.MEMBERSHIP_CHANGE }
                val groups = membership.map { it.conversationId }.toSet()
                assertEquals("One membership event per physical DM", membership.size, groups.size)
                assertTrue("Required membership is present", groups.containsAll(required))
                for (message in messages) {
                    if (message.kind == MessageKind.MEMBERSHIP_CHANGE) {
                        val expected = checkNotNull(physical[message.conversationId])
                        val update = (message.data.content as MessageContent.GroupUpdated).v1
                        assertEquals(GroupUpdatedCodec().type, message.contentType)
                        assertEquals(expected.creator, message.senderInboxId)
                        assertEquals(expected.creator, update.initiatedByInboxId)
                        assertEquals(listOf(expected.peer), update.addedInboxes)
                    } else {
                        assertEquals(MessageKind.APPLICATION, message.kind)
                        val expected = checkNotNull(applications[message.id])
                        assertEquals(expected.body, text(message))
                        assertEquals(expected.sender, message.senderInboxId)
                        assertEquals(expected.group, message.conversationId)
                    }
                }
            }

            assertHistory(boDm.messages(), setOf(boDm.id()))
            assertHistory(alixDm.messages(), setOf(alixDm.id()))
            bo.conversations().syncAll(null)
            alix.conversations().syncAll(null)
            assertEquals(physical.size, boDm.messages().size)
            assertEquals(physical.size, alixDm.messages().size)
            assertHistory(boDm.messages(), physical.keys)
            assertHistory(alixDm.messages(), physical.keys)
            val sameBo = alix.conversations().createDm(bo.inboxId())
            val sameAlix = bo.conversations().createDm(alix.inboxId())
            val byBoTopic =
                bo
                    .conversations()
                    .list(ListConversationsOptions(includeDuplicateDms = true))
                    .filterIsInstance<Conversation.Dm>()
                    .single { it.dm.topic() == boDm.topic() }
                    .dm
            val byAlixTopic =
                alix
                    .conversations()
                    .list(ListConversationsOptions(includeDuplicateDms = true))
                    .filterIsInstance<Conversation.Dm>()
                    .single { it.dm.topic() == alixDm.topic() }
                    .dm
            assertEquals(alixDm.id(), sameBo.id())
            assertEquals(alixDm.id(), sameAlix.id())
            assertEquals(boDm.id(), byBoTopic.id())
            assertEquals(alixDm.id(), byAlixTopic.id())
            assertEquals(
                alixDm.id(),
                alix
                    .conversations()
                    .listDms(null)
                    .first()
                    .id(),
            )
            assertEquals(
                alixDm.id(),
                bo
                    .conversations()
                    .listDms(null)
                    .first()
                    .id(),
            )
            val firstId = sameBo.sendText("Bo hey2")
            val secondId = sameAlix.sendText("Alix hey2")
            applications[firstId] = ExpectedApplication(alix.inboxId(), "Bo hey2", sameBo.id())
            applications[secondId] = ExpectedApplication(bo.inboxId(), "Alix hey2", sameAlix.id())
            assertEquals(2, applications.size)
            sameBo.sync()
            sameAlix.sync()
            assertEquals(physical.size + 2, sameBo.messages().size)
            assertEquals(physical.size + 2, sameAlix.messages().size)
            assertHistory(sameBo.messages(), physical.keys)
            assertHistory(sameAlix.messages(), physical.keys)
        }

    @Test fun testCanCreateADmWithInboxId() =
        runBlocking {
            val dm = bo.conversations().createDm(fixtures.alix)
            alix.conversations().sync()
            assertEquals(dm.id(), alix.conversations().createDm(fixtures.bo).id())
        }

    @Test fun testsCanFindDmByInboxId() =
        runBlocking {
            val dm = bo.conversations().createDm(caro.inboxId())
            assertNull(bo.conversations().getDmByInboxId(alix.inboxId()))
            assertEquals(dm.id(), bo.conversations().getDmByInboxId(caro.inboxId())?.id())
        }

    @Test fun testsCanFindDmByIdentity() =
        runBlocking {
            val dm = bo.conversations().createDm(caro.inboxId())
            assertNull(bo.conversations().getDmByIdentity(fixtures.alix))
            assertEquals(dm.id(), bo.conversations().getDmByIdentity(fixtures.caro)?.id())
        }

    @Test fun testCanListDmMembers() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            val expected = setOf(alix.inboxId(), bo.inboxId())
            assertEquals(expected, dm.members().map { it.inboxId }.toSet())
            assertEquals(
                expected,
                Conversation
                    .Dm(dm)
                    .members()
                    .map { it.inboxId }
                    .toSet(),
            )
            assertEquals(alix.inboxId(), dm.peerInboxId())
        }

    @Test fun testCannotCreateDmWithMemberNotOnV3() =
        runBlocking {
            val unregistered = createWallet().identity()
            try {
                bo.conversations().createDm(unregistered)
                fail("An unregistered identity must fail")
            } catch (_: XmtpException) {
                assertNull(bo.conversations().getDmByIdentity(unregistered))
            }
        }

    @Test fun testCannotStartDmWithSelf() =
        runBlocking {
            try {
                bo.conversations().createDm(bo.inboxId())
                fail("Recipient is sender")
            } catch (_: XmtpException) {
                assertTrue(bo.conversations().listDms(null).isEmpty())
            }
        }

    @Test fun testCannotStartDmWithAddressWhenExpectingInboxId() =
        runBlocking {
            try {
                bo.conversations().createDm(fixtures.alix.identifier)
                fail("An account address is not an inbox ID")
            } catch (_: XmtpException) {
                assertTrue(bo.conversations().listDms(null).isEmpty())
            }
        }

    @Test fun testDmStartsWithAllowedState() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("howdy")
            dm.sendText("gm")
            dm.sync()
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(ConsentEntity.Conversation(dm.id())))
            assertEquals(ConsentState.ALLOWED, dm.state().consentState)
        }

    @Test fun testsCanListDmsFiltered() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            bo.conversations().createGroup(listOf(caro.inboxId()))
            val dm = bo.conversations().createDm(alix.inboxId())
            assertEquals(2, bo.conversations().listDms(null).size)

            fun options(vararg states: ConsentState) = ListConversationsOptions(consentStates = states.toList())
            assertEquals(2, bo.conversations().listDms(options(ConsentState.ALLOWED)).size)
            dm.updateConsentState(ConsentState.DENIED)
            assertEquals(1, bo.conversations().listDms(options(ConsentState.ALLOWED)).size)
            assertEquals(1, bo.conversations().listDms(options(ConsentState.DENIED)).size)
            assertEquals(2, bo.conversations().listDms(options(ConsentState.ALLOWED, ConsentState.DENIED)).size)
            assertEquals(1, bo.conversations().listDms(null).size)
        }

    @Test fun testCanListDmsOrder() =
        runBlocking {
            val first = bo.conversations().createDm(caro.inboxId())
            val second = bo.conversations().createDm(alix.inboxId())
            val group = bo.conversations().createGroup(listOf(caro.inboxId()))
            second.sendText("Howdy")
            group.sendText("Howdy")
            bo.conversations().syncAll(null)
            assertEquals(listOf(second.id(), first.id()), bo.conversations().listDms(null).map { it.id() })
        }

    @Test fun testCanSendMessageToDm() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("howdy")
            val id = dm.sendText("gm")
            dm.sync()
            assertEquals("gm", text(dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
            assertEquals(id, dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().id)
            assertEquals(
                DeliveryStatus.PUBLISHED,
                dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().deliveryStatus,
            )
            assertEquals(3, dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            alix.conversations().sync()
            val peer = find(alix, bo.inboxId())
            peer.sync()
            assertEquals(3, peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            assertEquals("gm", text(peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
        }

    @Test fun testCanListDmMessages() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("howdy")
            dm.sendText("gm")
            val published = ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)
            assertEquals(3, dm.messages().size)
            assertEquals(3, dm.messages(published).size)
            dm.sync()
            assertEquals(3, dm.messages().size)
            assertEquals(0, dm.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)).size)
            assertEquals(3, dm.messages(published).size)
            alix.conversations().sync()
            val peer = find(alix, bo.inboxId())
            peer.sync()
            assertEquals(3, peer.messages(published).size)
        }

    @Test fun testCanSendContentTypesToDm() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            val parent = dm.sendText("gm")
            val reaction = Reaction("U+1F603", ReactionAction.ADDED, ReactionSchema.UNICODE)
            val id = dm.sendReaction(parent, bo.inboxId(), reaction)
            dm.sync()
            val messages = dm.messageHistorySnapshot(10u).messages
            assertEquals(3, messages.size)
            val content = (messages.single { it.id == id }.data.content as MessageContent.Reaction)
            assertEquals(parent, content.reference)
            assertEquals(reaction, content.reaction)
        }

    @Test fun testCanStreamDmMessages() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            alix.conversations().sync()
            val peer = checkNotNull(alix.conversations().getDmByIdentity(fixtures.bo))
            dm.sync()
            val retained = dm.messageHistorySnapshot(10u).messages
            assertEquals(1, retained.size)
            assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages(dm).collect { messages.add(it) } }
            try {
                messages.awaitHistory(retained)
                val first = peer.sendText("hi")
                messages.awaitApplications(listOf(first to "hi"))
                val second = peer.sendText("hi again")
                messages.awaitApplications(listOf(first to "hi", second to "hi again"))
                messages.awaitHistory(dm.messageHistorySnapshot(10u).messages)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamAllMessages() =
        runBlocking {
            val boDm = bo.conversations().createDm(alix.inboxId())
            alix.conversations().sync()
            val options = MessageReaderOptions(conversationKind = ConversationKind.DM)
            val messages = StreamTestMessages()
            val expected = mutableListOf<Pair<MessageId, String>>()
            val job = launch(Dispatchers.IO) { alix.messages(options).collect { messages.add(it) } }
            try {
                val retained = alix.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(1, retained.size)
                assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
                messages.awaitHistory(retained)
                repeat(2) {
                    val body = "Bo Message $it"
                    expected.add(boDm.sendText(body) to body)
                    messages.awaitApplications(expected)
                }
                val caroDm = caro.conversations().createDm(alix.inboxId())
                repeat(2) {
                    val body = "Caro Message $it"
                    expected.add(caroDm.sendText(body) to body)
                    messages.awaitApplications(expected)
                }
                val history = alix.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(6, history.size)
                assertEquals(
                    setOf(boDm.id(), caroDm.id()),
                    history
                        .filter {
                            it.kind == MessageKind.MEMBERSHIP_CHANGE
                        }.map { it.conversationId }
                        .toSet(),
                )
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamConversations() =
        runBlocking {
            val reader = bo.conversations().conversationReader(ConversationReaderOptions(kind = ConversationKind.DM))
            val received = Channel<ConversationId>(Channel.UNLIMITED)
            val closed = CompletableDeferred<Unit>()
            val job =
                launch(Dispatchers.IO) {
                    try {
                        while (true) {
                            val value = reader.next() ?: break
                            received.send((value as Conversation.Dm).dm.id())
                        }
                    } finally {
                        withContext(NonCancellable) { reader.end() }
                        closed.complete(Unit)
                    }
                }
            try {
                val first = alix.conversations().createDm(bo.inboxId())
                assertEquals(first.id(), withTimeout(3000) { received.receive() })
                val second = caro.conversations().createDm(bo.inboxId())
                assertEquals(second.id(), withTimeout(3000) { received.receive() })
                assertTrue("Unexpected conversation", received.tryReceive().isFailure)
            } finally {
                withContext(NonCancellable) {
                    withTimeout(30_000) {
                        job.cancelAndJoin()
                        closed.await()
                    }
                    received.cancel()
                }
            }
        }

    @Test fun testDmConsent() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            val entity = ConsentEntity.Conversation(dm.id())
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(entity))
            assertEquals(ConsentState.ALLOWED, dm.state().consentState)
            for (state in listOf(ConsentState.DENIED, ConsentState.ALLOWED)) {
                bo.preferences().setConsentStates(listOf(ConsentRecord(entity, state)))
                assertEquals(state, bo.preferences().consentState(entity))
                assertEquals(state, dm.state().consentState)
            }
        }

    @Test fun testCanGetLastReadTimes() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("Hello from Bo")
            dm.sync()
            alix.conversations().sync()
            val peer = find(alix, bo.inboxId())
            peer.sync()
            val id = peer.sendReadReceipt()
            dm.sync()
            assertEquals(
                dm
                    .messageHistorySnapshot(10u)
                    .messages
                    .single { it.id == id }
                    .sentAt,
                dm.lastReadTimes()[alix.inboxId()],
            )
        }

    @Test fun testDmDisappearingMessages() =
        runBlocking {
            val initial = DisappearingSettings(Timestamp(1_000_000_000), 1_000_000_000)
            val dm = bo.conversations().createDm(alix.inboxId(), CreateDmOptions(initial))
            dm.sendText("howdy")
            alix.conversations().syncAll(null)
            val peer = find(alix, bo.inboxId())
            assertEquals(2, dm.messages().size)
            assertEquals(2, peer.messages().size)
            assertEquals(initial, dm.state().disappearingSettings)
            delay(5000)
            assertEquals(1, dm.messages().size)
            assertEquals(1, peer.messages().size)
            dm.updateDisappearingSettings(null)
            dm.sync()
            peer.sync()
            assertEquals(DisappearingSettings(Timestamp(0), 0), dm.state().disappearingSettings)
            assertEquals(DisappearingSettings(Timestamp(0), 0), peer.state().disappearingSettings)
            assertFalse(dm.state().isDisappearingEnabled)
            assertFalse(peer.state().isDisappearingEnabled)
            dm.sendText("message after disabling disappearing")
            peer.sendText("another message after disabling")
            dm.sync()
            delay(1000)
            assertEquals(5, dm.messages().size)
            assertEquals(5, peer.messages().size)
            val updated =
                DisappearingSettings(
                    Timestamp(
                        dm
                            .messages(ListMessagesOptions(direction = MessageOrder.DESCENDING))
                            .first()
                            .sentAt.ns + 1_000_000_000,
                    ),
                    1_000_000_000,
                )
            dm.updateDisappearingSettings(updated)
            dm.sync()
            peer.sync()
            delay(1000)
            assertEquals(updated, dm.state().disappearingSettings)
            assertEquals(updated, peer.state().disappearingSettings)
            val first = dm.sendText("this will disappear soon")
            val second = peer.sendText("so will this")
            dm.sync()
            assertEquals(9, dm.messages().size)
            assertEquals(9, peer.messages().size)
            delay(6000)
            assertEquals(7, dm.messages().size)
            assertEquals(7, peer.messages().size)
            assertTrue(dm.messages().none { it.id == first || it.id == second })
            assertTrue(peer.messages().none { it.id == first || it.id == second })
            assertEquals(updated, dm.state().disappearingSettings)
            assertEquals(updated, peer.state().disappearingSettings)
            assertTrue(dm.state().isDisappearingEnabled)
            assertTrue(peer.state().isDisappearingEnabled)
        }

    @Test fun testCanQueryMessagesByInsertedTime() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            dm.sendText("first")
            dm.sendText("second")
            dm.sync()
            val messages = dm.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING))
            assertEquals(3, messages.size)
            val boundary = messages.last().insertedAt
            assertTrue(boundary.ns > 0)
            val filtered = dm.messages(ListMessagesOptions(insertedAfter = boundary))
            assertEquals(2, filtered.size)
            assertTrue(filtered.all { it.insertedAt.ns > boundary.ns })
            val bySent = dm.messages(ListMessagesOptions(sortBy = MessageSortBy.SENT_AT))
            val byInserted = dm.messages(ListMessagesOptions(sortBy = MessageSortBy.INSERTED_AT))
            assertEquals(bySent.size, byInserted.size)
            assertEquals(bySent.map { it.id }.toSet(), byInserted.map { it.id }.toSet())
        }

    @Test fun testCountMessagesWithExcludedContentTypes() =
        runBlocking {
            val dm = bo.conversations().createDm(alix.inboxId())
            val parent = dm.sendText("gm")
            dm.sync()
            val before = dm.countMessages(null)
            dm.sendReaction(parent, bo.inboxId(), Reaction("U+1F603", ReactionAction.ADDED, ReactionSchema.UNICODE))
            assertEquals(before + 1uL, dm.countMessages(null))
            val reactionTypes =
                listOf(ContentTypeId("xmtp.org", "reaction", 1u, 0u), ContentTypeId("xmtp.org", "reaction", 2u, 0u))
            assertEquals(before, dm.countMessages(ListMessagesOptions(excludeContentTypes = reactionTypes)))
        }
}
