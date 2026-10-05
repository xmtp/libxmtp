package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.collect
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class GroupTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private suspend fun group(
        members: List<InboxId> = listOf(alix.inboxId()),
        options: CreateGroupOptions? = null,
    ) = bo.conversations().createGroup(members, options)

    private suspend fun find(
        client: SDKClient,
        id: ConversationId,
    ): Group = (checkNotNull(client.conversations().getById(id)) as Conversation.Group).group

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    private suspend fun rejected(action: suspend () -> Unit) {
        try {
            action()
            fail("The operation must fail")
        } catch (_: XmtpException) {
        }
    }

    private suspend fun defaultPermissions(group: Group) {
        alix.conversations().sync()
        val peer = find(alix, group.id())
        assertTrue(group.id().isNotEmpty())
        assertTrue(peer.id().isNotEmpty())
        peer.addMembers(listOf(caro.inboxId()))
        group.sync()
        assertEquals(3, peer.members().size)
        assertEquals(3, group.members().size)
        rejected { peer.removeMembers(listOf(caro.inboxId())) }
        group.sync()
        assertEquals(3, peer.members().size)
        assertEquals(3, group.members().size)
        assertEquals(
            PermissionPolicy.ALLOW,
            group
                .state()
                .permissions.policySet.addMember,
        )
        assertEquals(
            PermissionPolicy.ALLOW,
            peer
                .state()
                .permissions.policySet.addMember,
        )
        assertTrue(group.isSuperAdmin(bo.inboxId()))
        assertFalse(group.isSuperAdmin(alix.inboxId()))
        assertTrue(peer.isSuperAdmin(bo.inboxId()))
        assertFalse(peer.isSuperAdmin(alix.inboxId()))
        assertTrue(group.isCreator())
        assertFalse(peer.isCreator())
    }

    @Test fun testCanCreateAGroupWithDefaultPermissions() =
        runBlocking {
            defaultPermissions(group())
        }

    @Test fun testCanCreateAGroupWithAdminPermissions() =
        runBlocking {
            val original = group(options = CreateGroupOptions(permissions = GroupPermissionMode.AdminOnly))
            alix.conversations().sync()
            val peer = find(alix, original.id())
            assertTrue(original.id().isNotEmpty())
            assertTrue(peer.id().isNotEmpty())
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(ConsentEntity.Conversation(original.id())))
            assertEquals(ConsentState.UNKNOWN, alix.preferences().consentState(ConsentEntity.Conversation(peer.id())))
            original.addMembers(listOf(caro.inboxId()))
            peer.sync()
            assertEquals(3, peer.members().size)
            assertEquals(3, original.members().size)
            rejected { peer.removeMembers(listOf(caro.inboxId())) }
            original.sync()
            assertEquals(3, peer.members().size)
            assertEquals(3, original.members().size)
            original.removeMembers(listOf(caro.inboxId()))
            peer.sync()
            assertEquals(2, peer.members().size)
            assertEquals(2, original.members().size)
            rejected { peer.addMembers(listOf(caro.inboxId())) }
            original.sync()
            assertEquals(2, peer.members().size)
            assertEquals(2, original.members().size)
            assertEquals(
                PermissionPolicy.ADMIN,
                original
                    .state()
                    .permissions.policySet.addMember,
            )
            assertEquals(
                PermissionPolicy.ADMIN,
                peer
                    .state()
                    .permissions.policySet.addMember,
            )
            assertTrue(original.isSuperAdmin(bo.inboxId()))
            assertFalse(original.isSuperAdmin(alix.inboxId()))
            assertTrue(peer.isSuperAdmin(bo.inboxId()))
            assertFalse(peer.isSuperAdmin(alix.inboxId()))
            assertFalse(peer.isCreator())
        }

    @Test fun testCanCreateAGroupWithInboxIdsDefaultPermissions() =
        runBlocking {
            defaultPermissions(bo.conversations().createGroup(listOf(fixtures.alix)))
        }

    @Test fun testCanListGroupMembers() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            assertEquals(
                setOf(alix.inboxId(), bo.inboxId(), caro.inboxId()),
                group.members().map { it.inboxId }.toSet(),
            )
            assertEquals(setOf(alix.inboxId(), caro.inboxId()), group.peerInboxIds().toSet())
        }

    @Test fun testGroupMetadata() =
        runBlocking {
            val group = group(options = CreateGroupOptions(name = "Starting Name", imageUrl = "startingurl.com"))
            assertEquals("Starting Name", group.state().name)
            assertEquals("startingurl.com", group.state().imageUrl)
            group.updateName("This Is A Great Group")
            group.updateImageUrl("thisisanewurl.com")
            group.sync()
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.sync()
            for (value in listOf(group, peer)) {
                assertEquals("This Is A Great Group", value.state().name)
                assertEquals("thisisanewurl.com", value.state().imageUrl)
            }
        }

    @Test fun testCanAddGroupMembers() =
        runBlocking {
            val group = group()
            assertEquals(caro.inboxId(), group.addMembers(listOf(caro.inboxId())).added.single())
            assertEquals(
                setOf(alix.inboxId(), bo.inboxId(), caro.inboxId()),
                group.members().map { it.inboxId }.toSet(),
            )
        }

    @Test fun testCannotStartGroupOrAddMembersWithAddressWhenExpectingInboxId() =
        runBlocking {
            rejected { group(listOf(fixtures.alix.identifier)) }
            val group = group()
            rejected { group.addMembers(listOf(fixtures.caro.identifier)) }
            rejected { group.removeMembers(listOf(fixtures.alix.identifier)) }
            assertEquals(setOf(alix.inboxId(), bo.inboxId()), group.members().map { it.inboxId }.toSet())
        }

    @Test fun testCanRemoveGroupMembers() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            group.removeMembers(listOf(caro.inboxId()))
            assertEquals(setOf(alix.inboxId(), bo.inboxId()), group.members().map { it.inboxId }.toSet())
        }

    @Test fun testCanRemoveGroupMembersWhenNotCreator() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            group.addAdmin(alix.inboxId())
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.removeMembers(listOf(caro.inboxId()))
            peer.sync()
            group.sync()
            assertFalse(peer.isCreator())
            assertEquals(setOf(alix.inboxId(), bo.inboxId()), group.members().map { it.inboxId }.toSet())
        }

    @Test fun testCanAddGroupMemberIds() =
        runBlocking {
            val group = group()
            assertEquals(caro.inboxId(), group.addMembers(listOf(fixtures.caro)).added.single())
            assertEquals(
                setOf(alix.inboxId(), bo.inboxId(), caro.inboxId()),
                group.members().map { it.inboxId }.toSet(),
            )
        }

    @Test fun testCanRemoveGroupMemberIds() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            group.removeMembers(listOf(fixtures.caro))
            assertEquals(setOf(alix.inboxId(), bo.inboxId()), group.members().map { it.inboxId }.toSet())
        }

    @Test fun testMessageTimeIsCorrect() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            group.sendText("Hello")
            assertEquals(2, group.messages().size)
            group.sync()
            val before = group.messages().last()
            group.sync()
            val after = group.messages().last()
            assertEquals(before.id, after.id)
            assertEquals(before.sentAt, after.sentAt)
        }

    @Test fun testIsActiveReturnsCorrectly() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            caro.conversations().sync()
            val peer = find(caro, group.id())
            peer.sync()
            assertTrue(peer.state().common.isActive)
            assertTrue(group.state().common.isActive)
            group.removeMembers(listOf(caro.inboxId()))
            peer.sync()
            assertTrue(group.state().common.isActive)
            assertFalse(peer.state().common.isActive)
        }

    @Test fun testAddedByAddress() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            bo.conversations().sync()
            assertEquals(alix.inboxId(), find(bo, group.id()).addedByInboxId())
        }

    @Test fun testCanListGroups() =
        runBlocking {
            group()
            group(listOf(caro.inboxId()))
            bo.conversations().sync()
            assertEquals(2, bo.conversations().listGroups(null).size)
        }

    @Test fun testCanListGroupsAndConversations() =
        runBlocking {
            group()
            group(listOf(caro.inboxId()))
            bo.conversations().createDm(alix.inboxId())
            bo.conversations().sync()
            assertEquals(3, bo.conversations().list().size)
        }

    @Test fun testCannotSendMessageToGroupMemberNotOnV3() =
        runBlocking {
            val identity = createWallet().identity()
            rejected { bo.conversations().createGroup(listOf(identity)) }
        }

    @Test fun testCanStartEmptyGroupChat() =
        runBlocking {
            assertTrue(group(emptyList()).id().isNotEmpty())
        }

    @Test fun testGroupStartsWithAllowedState() =
        runBlocking {
            val group = group()
            group.sendText("howdy")
            group.sendText("gm")
            group.sync()
            assertEquals(ConsentState.ALLOWED, group.state().common.consentState)
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(ConsentEntity.Conversation(group.id())))
        }

    @Test fun testCanStreamAndUpdateNameWithoutForkingGroup() =
        runBlocking {
            val messages = StreamTestMessages()
            val expected = mutableListOf<Pair<MessageId, String>>()
            val job = launch(Dispatchers.IO) { bo.messages().collect { messages.add(it) } }
            try {
                val original = alix.conversations().createGroup(listOf(bo.inboxId()))
                expected.add(original.sendText("hello1") to "hello1")
                messages.awaitApplications(expected)
                original.updateName("hello")
                bo.conversations().sync()
                val groups = bo.conversations().listGroups(null)
                assertEquals(1, groups.size)
                val peer = groups.single()
                peer.sync()
                assertEquals(3, peer.messages().size)
                assertEquals("hello", peer.state().name)
                expected.add(peer.sendText("hello2") to "hello2")
                messages.awaitApplications(expected)
                expected.add(peer.sendText("hello3") to "hello3")
                messages.awaitApplications(expected)
                withTimeout(30_000) {
                    while (original.messages().size < 5) {
                        original.sync()
                        delay(100)
                    }
                }
                assertEquals(5, original.messages().size)
                expected.add(original.sendText("hello4") to "hello4")
                messages.awaitApplications(expected)
                peer.sync()
                val history = peer.messageHistorySnapshot(10u).messages
                assertEquals(6, history.size)
                assertEquals(2, history.count { it.kind == MessageKind.MEMBERSHIP_CHANGE })
                messages.awaitHistory(history)
                assertFalse(peer.debugInfo().maybeForked)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testsCanListGroupsFiltered() =
        runBlocking {
            bo.conversations().createDm(caro.inboxId())
            group(listOf(caro.inboxId()))
            val group = group(listOf(caro.inboxId()))

            fun options(vararg states: ConsentState) = ListConversationsOptions(consentStates = states.toList())
            assertEquals(2, bo.conversations().listGroups(null).size)
            assertEquals(2, bo.conversations().listGroups(options(ConsentState.ALLOWED)).size)
            group.updateConsentState(ConsentState.DENIED)
            assertEquals(1, bo.conversations().listGroups(options(ConsentState.ALLOWED)).size)
            assertEquals(1, bo.conversations().listGroups(options(ConsentState.DENIED)).size)
            assertEquals(2, bo.conversations().listGroups(options(ConsentState.ALLOWED, ConsentState.DENIED)).size)
            assertEquals(1, bo.conversations().listGroups(null).size)
        }

    @Test fun testCanListGroupsOrder() =
        runBlocking {
            val dm = bo.conversations().createDm(caro.inboxId())
            val first = group(listOf(caro.inboxId()))
            val second = group(listOf(caro.inboxId()))
            dm.sendText("Howdy")
            second.sendText("Howdy")
            bo.conversations().syncAll(null)
            assertEquals(listOf(second.id(), first.id()), bo.conversations().listGroups(null).map { it.id() })
        }

    @Test fun testCanSendMessageToGroup() =
        runBlocking {
            val group = group()
            group.sendText("howdy")
            val id = group.sendText("gm")
            group.sync()
            assertEquals("gm", text(group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
            assertEquals(id, group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().id)
            assertEquals(
                DeliveryStatus.PUBLISHED,
                group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().deliveryStatus,
            )
            assertEquals(3, group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.sync()
            assertEquals(3, peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).size)
            assertEquals("gm", text(peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first()))
        }

    @Test fun testCanListGroupMessages() =
        runBlocking {
            val group = group()
            group.sendText("howdy")
            group.sendText("gm")
            val published = ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)
            assertEquals(3, group.messages().size)
            assertEquals(3, group.messages(published).size)
            group.sync()
            assertEquals(3, group.messages().size)
            assertEquals(0, group.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)).size)
            assertEquals(3, group.messages(published).size)
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.sync()
            assertEquals(3, peer.messages(published).size)
        }

    @Test fun testCanListGroupMessagesAfter() =
        runBlocking {
            val group = group()
            group.sendText("howdy")
            val boundary = group.sendText("gm")
            val message = checkNotNull(bo.conversations().getMessageById(boundary))
            val options = ListMessagesOptions(sentAfter = message.sentAt)
            assertEquals(3, group.messages().size)
            assertEquals(0, group.messages(options).size)
            group.sendText("howdy")
            group.sendText("gm")
            assertEquals(5, group.messages().size)
            assertEquals(2, group.messages(options).size)
            alix.conversations().sync()
            val peer = find(alix, group.id())
            peer.sync()
            assertEquals(5, peer.messages().size)
            assertEquals(2, peer.messages(options).size)
        }

    @Test fun testCanSendContentTypesToGroup() =
        runBlocking {
            val group = group()
            val parent = group.sendText("gm")
            val reaction = Reaction("U+1F603", ReactionAction.ADDED, ReactionSchema.UNICODE)
            val id = group.sendReaction(parent, bo.inboxId(), reaction)
            group.sync()
            val messages = group.messageHistorySnapshot(10u).messages
            assertEquals(3, messages.size)
            val body = messages.single { it.id == id }.data.content as MessageContent.Reaction
            assertEquals(parent, body.reference)
            assertEquals(reaction, body.reaction)
        }

    @Test fun testCanStreamGroupMessages() =
        runBlocking {
            val group = group()
            alix.conversations().sync()
            val peer = find(alix, group.id())
            val retained = group.messageHistorySnapshot(10u).messages
            assertEquals(1, retained.size)
            assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { bo.messages(group).collect { messages.add(it) } }
            try {
                messages.awaitHistory(retained)
                val first = peer.sendText("hi")
                messages.awaitApplications(listOf(first to "hi"))
                try {
                    peer.send(EncodedContent(GroupUpdatedCodec().type, content = byteArrayOf()))
                    fail("Applications cannot send reserved membership content")
                } catch (error: XmtpException.InvalidInput) {
                    assertEquals("ReservedTranscriptContentType", error.v1.code)
                    assertEquals(ErrorCategory.INPUT, error.v1.category)
                    assertFalse(error.v1.retryable)
                }
                val second = peer.sendText("hi again")
                messages.awaitApplications(listOf(first to "hi", second to "hi again"))
                val history = group.messages()
                assertEquals(3, history.size)
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun testCanStreamAllGroupMessages() =
        runBlocking {
            val first = caro.conversations().createGroup(listOf(alix.inboxId()))
            val dm = caro.conversations().createDm(alix.inboxId())
            alix.conversations().sync()
            val options = MessageReaderOptions(conversationKind = ConversationKind.GROUP)
            val messages = StreamTestMessages()
            val expected = mutableListOf<Pair<MessageId, String>>()
            val job = launch(Dispatchers.IO) { alix.messages(options).collect { messages.add(it) } }
            try {
                val retained = alix.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(1, retained.size)
                assertEquals(MessageKind.MEMBERSHIP_CHANGE, retained.single().kind)
                messages.awaitHistory(retained)
                val excluded = dm.sendText("conversation message")
                repeat(2) {
                    val body = "First group message $it"
                    expected.add(first.sendText(body) to body)
                    messages.awaitApplications(expected)
                }
                val second = caro.conversations().createGroup(listOf(alix.inboxId()))
                repeat(2) {
                    val body = "Second group message $it"
                    expected.add(second.sendText(body) to body)
                    messages.awaitApplications(expected)
                }
                val peerDm = checkNotNull(alix.conversations().getDmByInboxId(caro.inboxId()))
                peerDm.sync()
                assertTrue(peerDm.messages().any { it.id == excluded })
                delay(1000)
                val history = alix.conversations().messageHistorySnapshot(10u, options).messages
                assertEquals(6, history.size)
                assertEquals(
                    setOf(first.id(), second.id()),
                    history
                        .filter {
                            it.kind == MessageKind.MEMBERSHIP_CHANGE
                        }.map { it.conversationId }
                        .toSet(),
                )
                assertFalse(messages.snapshot().any { it.id == excluded || it.conversationId == dm.id() })
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    private suspend fun assertConversationStream(kind: ConversationKind?) =
        coroutineScope {
            val reader = alix.conversations().conversationReader(ConversationReaderOptions(kind = kind))
            val received = Channel<Pair<ConversationId, String>>(Channel.UNLIMITED)
            val closed = CompletableDeferred<Unit>()
            val job =
                launch(Dispatchers.IO) {
                    try {
                        while (true) {
                            val value = reader.next() ?: break
                            val row =
                                when (value) {
                                    is Conversation.Group -> value.group.id() to value.group.topic()
                                    is Conversation.Dm -> value.dm.id() to value.dm.topic()
                                }
                            received.send(row)
                        }
                    } finally {
                        withContext(NonCancellable) { reader.end() }
                        closed.complete(Unit)
                    }
                }
            try {
                val expected =
                    if (kind == null) {
                        val dm = alix.conversations().createDm(bo.inboxId())
                        val group = caro.conversations().createGroup(listOf(alix.inboxId()))
                        listOf(dm.id() to dm.topic(), group.id() to group.topic())
                    } else {
                        val first = bo.conversations().createGroup(listOf(alix.inboxId()))
                        val second = caro.conversations().createGroup(listOf(alix.inboxId()))
                        listOf(first.id() to first.topic(), second.id() to second.topic())
                    }
                val actual = List(2) { withTimeout(3000) { received.receive() } }
                assertEquals(expected, actual)
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

    @Test fun testCanStreamGroups() = runBlocking { assertConversationStream(ConversationKind.GROUP) }

    @Test fun testCanStreamGroupsAndConversations() = runBlocking { assertConversationStream(null) }

    @Test fun testGroupConsent() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            val entity = ConsentEntity.Conversation(group.id())
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(entity))
            assertEquals(ConsentState.ALLOWED, group.state().common.consentState)
            bo.preferences().setConsentStates(listOf(ConsentRecord(entity, ConsentState.DENIED)))
            assertEquals(ConsentState.DENIED, bo.preferences().consentState(entity))
            assertEquals(ConsentState.DENIED, group.state().common.consentState)
            group.updateConsentState(ConsentState.ALLOWED)
            assertEquals(ConsentState.ALLOWED, bo.preferences().consentState(entity))
            assertEquals(ConsentState.ALLOWED, group.state().common.consentState)
        }

    @Test fun testCanAllowAndDenyInboxId() =
        runBlocking {
            val group = group()
            val entity = ConsentEntity.Inbox(alix.inboxId())
            assertEquals(ConsentState.UNKNOWN, bo.preferences().consentState(entity))
            for (state in listOf(ConsentState.ALLOWED, ConsentState.DENIED)) {
                bo.preferences().setConsentStates(listOf(ConsentRecord(entity, state)))
                assertEquals(state, group.members().single { it.inboxId == alix.inboxId() }.consentState)
                assertEquals(state, bo.preferences().consentState(entity))
            }
        }

    @Test fun testCanFetchGroupById() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            alix.conversations().sync()
            assertEquals(group.id(), find(alix, group.id()).id())
        }

    @Test fun testCanFetchMessageById() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            val id = group.sendText("Hello")
            alix.conversations().sync()
            find(alix, group.id()).sync()
            assertEquals(id, alix.conversations().getMessageById(id)?.id)
        }

    @Test fun testUnpublishedMessages() =
        runBlocking {
            val group = group(listOf(alix.inboxId(), caro.inboxId()))
            alix.conversations().sync()
            val peer = find(alix, group.id())
            assertEquals(ConsentState.UNKNOWN, peer.state().common.consentState)
            val id = peer.prepareMessage(encodeText("Test text"))
            assertEquals(2, peer.messages().size)
            assertEquals(1, peer.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)).size)
            assertEquals(1, peer.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)).size)
            peer.publishMessages()
            peer.sync()
            assertEquals(ConsentState.ALLOWED, peer.state().common.consentState)
            assertEquals(2, peer.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED)).size)
            assertEquals(0, peer.messages(ListMessagesOptions(deliveryStatus = DeliveryStatus.UNPUBLISHED)).size)
            assertEquals(2, peer.messages().size)
            assertEquals(id, peer.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first().id)
        }

    @Test fun testSyncsAllGroupsInParallel() =
        runBlocking {
            val first = group()
            val second = group()
            alix.conversations().sync()
            val firstPeer = find(alix, first.id())
            val secondPeer = find(alix, second.id())
            assertEquals(1, firstPeer.messages().size)
            assertEquals(1, secondPeer.messages().size)
            first.sendText("hi")
            second.sendText("hi")
            var summary = alix.conversations().syncAll(null)
            assertEquals(2, firstPeer.messages().size)
            assertEquals(2, secondPeer.messages().size)
            assertEquals(3uL, summary.eligible)
            second.removeMembers(listOf(alix.inboxId()))
            repeat(2) {
                first.sendText("hi")
                second.sendText("hi")
            }
            summary = alix.conversations().syncAll(null)
            delay(2000)
            assertEquals(4, firstPeer.messages().size)
            assertEquals(3, secondPeer.messages().size)
            assertEquals(3uL, summary.eligible)
            summary = alix.conversations().syncAll(null)
            assertEquals(3uL, summary.eligible)
            assertEquals(2uL, summary.synced)
            assertFalse(secondPeer.state().common.isActive)
            assertTrue(firstPeer.state().common.isActive)
        }

    @Test fun testGroupDisappearingMessages() =
        runBlocking {
            val initial = DisappearingSettings(Timestamp(1_000_000_000), 1_000_000_000)
            val group = group(options = CreateGroupOptions(disappearing = initial))
            group.sendText("howdy")
            alix.conversations().syncAll(null)
            val peer = find(alix, group.id())
            assertEquals(2, group.messages().size)
            assertEquals(2, peer.messages().size)
            assertEquals(initial, group.state().common.disappearingSettings)
            delay(5000)
            assertEquals(1, group.messages().size)
            assertEquals(1, peer.messages().size)
            group.updateDisappearingSettings(null)
            group.sync()
            peer.sync()
            assertEquals(DisappearingSettings(Timestamp(0), 0), group.state().common.disappearingSettings)
            assertEquals(DisappearingSettings(Timestamp(0), 0), peer.state().common.disappearingSettings)
            assertFalse(group.state().common.isDisappearingEnabled)
            assertFalse(peer.state().common.isDisappearingEnabled)
            group.sendText("message after disabling disappearing")
            peer.sendText("another message after disabling")
            group.sync()
            delay(1000)
            assertEquals(5, group.messages().size)
            assertEquals(5, peer.messages().size)
            val updated =
                DisappearingSettings(
                    Timestamp(
                        group
                            .messages(ListMessagesOptions(direction = MessageOrder.DESCENDING))
                            .first()
                            .sentAt.ns + 1_000_000_000,
                    ),
                    1_000_000_000,
                )
            group.updateDisappearingSettings(updated)
            group.sync()
            peer.sync()
            delay(2000)
            assertEquals(updated, group.state().common.disappearingSettings)
            assertEquals(updated, peer.state().common.disappearingSettings)
            val first = group.sendText("this will disappear soon")
            val second = peer.sendText("so will this")
            group.sync()
            assertEquals(9, group.messages().size)
            assertEquals(9, peer.messages().size)
            delay(6000)
            assertEquals(7, group.messages().size)
            assertEquals(7, peer.messages().size)
            assertTrue(group.messages().none { it.id == first || it.id == second })
            assertTrue(peer.messages().none { it.id == first || it.id == second })
            assertEquals(updated, group.state().common.disappearingSettings)
            assertEquals(updated, peer.state().common.disappearingSettings)
            assertTrue(group.state().common.isDisappearingEnabled)
            assertTrue(peer.state().common.isDisappearingEnabled)
        }

    @Test fun testGroupPausedForVersionReturnsNone() =
        runBlocking {
            assertNull(group().state().common.pausedForVersion)
            assertNull(
                bo
                    .conversations()
                    .createDm(alix.inboxId())
                    .state()
                    .pausedForVersion,
            )
        }

    @Test fun testCanQueryMessagesByInsertedTime() =
        runBlocking {
            val group = group()
            group.sendText("first")
            group.sendText("second")
            group.sync()
            val messages = group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING))
            assertEquals(3, messages.size)
            val boundary = messages.last().insertedAt
            assertTrue(boundary.ns > 0)
            val filtered = group.messages(ListMessagesOptions(insertedAfter = boundary))
            assertEquals(2, filtered.size)
            assertTrue(filtered.all { it.insertedAt.ns > boundary.ns })
            assertEquals(
                group.messages(ListMessagesOptions(sortBy = MessageSortBy.SENT_AT)).map { it.id }.toSet(),
                group.messages(ListMessagesOptions(sortBy = MessageSortBy.INSERTED_AT)).map { it.id }.toSet(),
            )
            assertEquals(2uL, group.countMessages(ListMessagesOptions(insertedAfter = boundary)))
        }

    @Test fun testCountMessagesWithExcludedContentTypes() =
        runBlocking {
            val group = group()
            val parent = group.sendText("gm")
            group.sync()
            group.sendReaction(parent, bo.inboxId(), Reaction("U+1F603", ReactionAction.ADDED, ReactionSchema.UNICODE))
            assertEquals(3uL, group.countMessages(null))
            val reactionTypes =
                listOf(ContentTypeId("xmtp.org", "reaction", 1u, 0u), ContentTypeId("xmtp.org", "reaction", 2u, 0u))
            assertEquals(2uL, group.countMessages(ListMessagesOptions(excludeContentTypes = reactionTypes)))
        }

    @Test fun testCanLeaveGroup() =
        runBlocking {
            val group = group()
            alix.conversations().syncAll(null)
            bo.conversations().syncAll(null)
            val peer = find(alix, group.id())
            assertEquals(2, group.members().size)
            assertTrue(peer.state().common.isActive)
            peer.requestRemoval()
            peer.sync()
            group.sync()
            assertTrue(peer.state().common.isActive)
            delay(3000)
            assertEquals(1, group.members().size)
            peer.sync()
            assertFalse(peer.state().common.isActive)
        }

    @Test fun testSelfRemovalWithMembershipState() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            bo.conversations().sync()
            val peer = find(bo, group.id())
            assertEquals(MembershipState.PENDING, peer.state().membershipState)
            assertEquals(MembershipState.ALLOWED, group.state().membershipState)
            peer.requestRemoval()
            assertEquals(MembershipState.PENDING_REMOVE, peer.state().membershipState)
            group.sync()
            delay(2000)
            peer.sync()
            assertFalse(peer.state().common.isActive)
            assertEquals(MembershipState.ALLOWED, group.state().membershipState)
            group.sync()
            assertEquals("Only the creator remains", 1, group.members().size)
        }
}
