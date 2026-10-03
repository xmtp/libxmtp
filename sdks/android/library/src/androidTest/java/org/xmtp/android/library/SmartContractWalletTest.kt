package org.xmtp.android.library

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class SmartContractWalletTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var davonSCW: FakeSCWWallet
    private lateinit var eriSCW: FakeSCWWallet
    private lateinit var davon: SDKClient
    private lateinit var eri: SDKClient
    private lateinit var boSigner: Signer
    private lateinit var bo: SDKClient

    @Before override fun setUp() {
        super.setUp()
        runBlocking {
            fixtures = createFixtures()
            boSigner = createWallet()
            bo = createClient(boSigner)
            davonSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_1)
            davon = createClient(davonSCW)
            eriSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_2)
            eri = createClient(eriSCW)
        }
    }

    @After fun closeWallets() {
        if (::davonSCW.isInitialized) davonSCW.close()
        if (::eriSCW.isInitialized) eriSCW.close()
    }

    private suspend fun group(
        client: SDKClient,
        id: ConversationId,
    ): Group = (checkNotNull(client.conversations().getById(id)) as Conversation.Group).group

    private fun text(message: Message): String? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.Text)?.v1

    @Test fun test1_CanBuildASCW() =
        runBlocking {
            val inbox = davon.inboxId()
            val installation = davon.installationId()
            val path = davon.storagePath()
            val identity = davonSCW.identity()
            val options = davon.options()
            davon.end()
            val reopened = trackClient(SDKClient.build(context, identity, options, inbox))
            assertEquals(inbox, reopened.inboxId())
            assertEquals(installation, reopened.installationId())
            assertEquals(path, reopened.storagePath())
            assertEquals(inbox, reopened.inboxIdFor(identity))
            assertEquals(true, reopened.canMessage(listOf(boSigner.identity()))[boSigner.identity().identifier])
            assertEquals(true, bo.canMessage(listOf(identity))[identity.identifier])
        }

    @Test fun test2_CanCreateGroup() =
        runBlocking {
            val first = bo.conversations().createGroup(listOf(davon.inboxId(), eri.inboxId()))
            val second = davon.conversations().createGroup(listOf(bo.inboxId(), eri.inboxId()))
            assertEquals(
                setOf(davon.inboxId(), bo.inboxId(), eri.inboxId()),
                first.members().map { it.inboxId }.toSet(),
            )
            assertEquals(
                setOf(davonSCW.identity().identifier, boSigner.identity().identifier, eriSCW.identity().identifier),
                second
                    .members()
                    .flatMap { it.identities }
                    .map { it.identifier }
                    .toSet(),
            )
        }

    @Test fun test3_CanSendMessages() =
        runBlocking {
            val original = bo.conversations().createGroup(listOf(davon.inboxId(), eri.inboxId()))
            original.sendText("howdy")
            val id = original.sendText("gm")
            original.sync()
            val latest = original.messages().first()
            assertEquals("gm", text(latest))
            assertEquals(id, latest.id)
            assertEquals(DeliveryStatus.PUBLISHED, latest.deliveryStatus)
            assertEquals(3, original.messages().size)
            davon.conversations().syncAll(null)
            val davonGroup = group(davon, original.id())
            assertEquals(3, davonGroup.messages().size)
            assertEquals("gm", text(davonGroup.messages().first()))
            davonGroup.sendText("from davon")
            eri.conversations().syncAll(null)
            val eriGroup = group(eri, original.id())
            eriGroup.sync()
            assertEquals(4, eriGroup.messages().size)
            assertEquals("from davon", text(eriGroup.messages().first()))
            val eriId = eriGroup.sendText("from eri")
            original.sync()
            assertEquals(eri.inboxId(), original.messages().single { it.id == eriId }.senderInboxId)
        }

    @Test fun test4_GroupConsent() =
        runBlocking {
            val group = davon.conversations().createGroup(listOf(bo.inboxId(), eri.inboxId()))
            val entity = ConsentEntity.Conversation(group.id())
            assertEquals(ConsentState.ALLOWED, davon.preferences().consentState(entity))
            assertEquals(ConsentState.ALLOWED, group.state().common.consentState)
            davon.preferences().setConsentStates(listOf(ConsentRecord(entity, ConsentState.DENIED)))
            assertEquals(ConsentState.DENIED, davon.preferences().consentState(entity))
            assertEquals(ConsentState.DENIED, group.state().common.consentState)
            group.updateConsentState(ConsentState.ALLOWED)
            assertEquals(ConsentState.ALLOWED, davon.preferences().consentState(entity))
            assertEquals(ConsentState.ALLOWED, group.state().common.consentState)
        }

    @Test fun test5_CanAllowAndDenyInboxId() =
        runBlocking {
            val group = davon.conversations().createGroup(listOf(bo.inboxId(), eri.inboxId()))
            val entity = ConsentEntity.Inbox(bo.inboxId())
            assertEquals(ConsentState.UNKNOWN, davon.preferences().consentState(entity))
            for (state in listOf(ConsentState.ALLOWED, ConsentState.DENIED)) {
                davon.preferences().setConsentStates(listOf(ConsentRecord(entity, state)))
                assertEquals(state, group.members().single { it.inboxId == bo.inboxId() }.consentState)
                assertEquals(state, davon.preferences().consentState(entity))
            }
        }

    @Test fun test6_CanStreamAllMessages() =
        runBlocking {
            val first = davon.conversations().createGroup(listOf(bo.inboxId(), eri.inboxId()))
            val second = bo.conversations().createGroup(listOf(davon.inboxId(), eri.inboxId()))
            val firstDm = davon.conversations().createDm(eri.inboxId())
            val secondDm = bo.conversations().createDm(davon.inboxId())
            davon.conversations().syncAll(null)
            val retained = davon.conversations().messageHistorySnapshot(10u).messages
            assertEquals(4, retained.size)
            assertEquals(
                setOf(first.id(), second.id(), firstDm.id(), secondDm.id()),
                retained.map { it.conversationId }.toSet(),
            )
            assertTrue(retained.all { it.kind == MessageKind.MEMBERSHIP_CHANGE })
            val messages = StreamTestMessages()
            val job = launch(Dispatchers.IO) { davon.messages().collect { messages.add(it) } }
            try {
                messages.awaitHistory(retained)
                val expected = mutableListOf(first.sendText("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(second.sendText("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(firstDm.sendText("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(secondDm.sendText("hi") to "hi")
                messages.awaitApplications(expected)
                val history = davon.conversations().messageHistorySnapshot(10u).messages
                assertEquals(retained.size + expected.size, history.size)
                assertEquals(
                    retained.map { it.id },
                    history.filter { it.kind == MessageKind.MEMBERSHIP_CHANGE }.map { it.id },
                )
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test fun test7_CanStreamConversations() =
        runBlocking {
            val reader = davon.conversations().conversationReader(null)
            val received = mutableListOf<Pair<String, String>>()
            val job =
                launch(Dispatchers.IO) {
                    while (true) {
                        val conversation = reader.next() ?: break
                        val row =
                            when (conversation) {
                                is Conversation.Group -> conversation.group.id() to conversation.group.topic()
                                is Conversation.Dm -> conversation.dm.id() to conversation.dm.topic()
                            }
                        synchronized(received) { received.add(row) }
                    }
                }
            try {
                val first = davon.conversations().createGroup(listOf(bo.inboxId(), eri.inboxId()))
                val second = bo.conversations().createGroup(listOf(davon.inboxId(), eri.inboxId()))
                val firstDm = davon.conversations().createDm(fixtures.alixClient.inboxId())
                val secondDm = fixtures.caroClient.conversations().createDm(davon.inboxId())
                val expected =
                    listOf(
                        first.id() to first.topic(),
                        second.id() to second.topic(),
                        firstDm.id() to firstDm.topic(),
                        secondDm.id() to secondDm.topic(),
                    )
                withTimeout(30_000) { while (synchronized(received) { received.size } < expected.size) delay(10) }
                assertEquals(
                    expected.sortedBy { it.first },
                    synchronized(received) { received.toList() }.sortedBy { it.first },
                )
            } finally {
                withContext(NonCancellable) {
                    job.cancelAndJoin()
                    reader.end()
                }
            }
        }

    @Test fun test8_AddAndRemovingAccounts() =
        runBlocking {
            val eoa = createWallet()
            FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_3).use { added ->
                davon.unsafeAddAccount(eoa, false)
                davon.unsafeAddAccount(added, false)
                val recovery = davonSCW.identity()
                var state = davon.inboxState(true)
                assertEquals(1, state.installations.size)
                assertEquals(setOf(recovery, eoa.identity(), added.identity()), state.identities.toSet())
                assertEquals(recovery, state.recoveryIdentity)
                davon.removeAccount(davonSCW, added.identity())
                state = davon.inboxState(true)
                assertEquals(1, state.installations.size)
                assertEquals(setOf(recovery, eoa.identity()), state.identities.toSet())
                assertEquals(recovery, state.recoveryIdentity)
                try {
                    davon.removeAccount(eoa, recovery)
                    fail("A non-recovery signer must not remove the recovery account")
                } catch (_: XmtpException) {
                    assertEquals(setOf(recovery, eoa.identity()), davon.inboxState(true).identities.toSet())
                }
            }
        }
}
