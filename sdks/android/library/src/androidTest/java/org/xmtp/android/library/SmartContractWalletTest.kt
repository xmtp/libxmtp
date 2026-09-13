package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiException
import java.io.File

@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class SmartContractWalletTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var davonSCW: FakeSCWWallet
    private lateinit var davonSCWClient: Client
    private lateinit var eriSCW: FakeSCWWallet
    private lateinit var eriSCWClient: Client
    private lateinit var boEOAWallet: PrivateKeyBuilder
    private lateinit var boEOA: PrivateKey
    private lateinit var boEOAClient: Client

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }

        // EOA
        boEOAWallet = createWallet()
        boEOA = boEOAWallet.getPrivateKey()
        boEOAClient = runBlocking { createClient(boEOAWallet) }

        // SCW
        davonSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_1)
        davonSCWClient =
            runBlocking { createClient(davonSCW) }

        // SCW
        eriSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_2)
        eriSCWClient = runBlocking { createClient(eriSCW) }
    }

    @Test
    fun test1_CanBuildASCW() {
        val davonSCWClient2 =
            runBlocking {
                Client.build(
                    publicIdentity = davonSCW.publicIdentity,
                    createClientOptions(
                        localApi(),
                        dbDirectory = File(davonSCWClient.dbPath).parent,
                        deviceSyncEnabled = false,
                    ),
                    davonSCWClient.inboxId,
                )
            }

        assertEquals(davonSCWClient.inboxId, davonSCWClient2.inboxId)
        assertEquals(
            davonSCWClient2.inboxId,
            runBlocking { davonSCWClient.inboxIdFromIdentity(davonSCW.publicIdentity) },
        )

        runBlocking {
            davonSCWClient
                .canMessage(listOf(boEOAWallet.publicIdentity))[
                boEOAWallet.publicIdentity.identifier,
            ]?.let { assert(it) }
        }

        runBlocking {
            boEOAClient
                .canMessage(listOf(davonSCW.publicIdentity))[
                davonSCW.publicIdentity.identifier,
            ]?.let { assert(it) }
        }
    }

    @Test
    fun test2_CanCreateGroup() {
        val group1 =
            runBlocking {
                boEOAClient.conversations.newGroup(listOf(davonSCWClient.inboxId, eriSCWClient.inboxId))
            }
        val group2 =
            runBlocking {
                davonSCWClient.conversations.newGroup(listOf(boEOAClient.inboxId, eriSCWClient.inboxId))
            }

        assertEquals(
            runBlocking { group1.members().map { it.inboxId }.sorted() },
            listOf(davonSCWClient.inboxId, boEOAClient.inboxId, eriSCWClient.inboxId).sorted(),
        )
        assertEquals(
            runBlocking { group2.members().map { it.identities.first().identifier }.sorted() },
            listOf(
                davonSCW.publicIdentity.identifier,
                boEOAWallet.publicIdentity.identifier,
                eriSCW.publicIdentity.identifier,
            ).sorted(),
        )
    }

    @Test
    fun test3_CanSendMessages() {
        val boGroup =
            runBlocking {
                boEOAClient.conversations.newGroup(listOf(davonSCWClient.inboxId, eriSCWClient.inboxId))
            }
        runBlocking { boGroup.send("howdy") }
        val messageId = runBlocking { boGroup.send("gm") }
        runBlocking { boGroup.sync() }
        assertEquals(runBlocking { boGroup.messages() }.first().body, "gm")
        assertEquals(runBlocking { boGroup.messages() }.first().id, messageId)
        assertEquals(
            runBlocking { boGroup.messages() }.first().deliveryStatus,
            DecodedMessage.MessageDeliveryStatus.PUBLISHED,
        )
        assertEquals(runBlocking { boGroup.messages() }.size, 3)

        runBlocking { davonSCWClient.conversations.sync() }
        val davonGroup = runBlocking { davonSCWClient.conversations.findGroup(boGroup.id)!! }
        runBlocking { davonGroup.sync() }
        assertEquals(runBlocking { davonGroup.messages() }.size, 3)
        assertEquals(runBlocking { davonGroup.messages() }.first().body, "gm")
        runBlocking { davonGroup.send("from davon") }

        runBlocking { eriSCWClient.conversations.sync() }
        val eriGroup = runBlocking { davonSCWClient.conversations.findGroup(davonGroup.id) }
        runBlocking { eriGroup?.sync() }
        assertEquals(runBlocking { eriGroup?.messages() }?.size, 4)
        assertEquals(runBlocking { eriGroup?.messages() }?.first()?.body, "from davon")
        runBlocking { eriGroup?.send("from eri") }
    }

    @Test
    fun test4_GroupConsent() {
        runBlocking {
            val davonGroup =
                runBlocking {
                    davonSCWClient.conversations.newGroup(
                        listOf(boEOAClient.inboxId, eriSCWClient.inboxId),
                    )
                }
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.ALLOWED,
            )
            assertEquals(davonGroup.consentState(), ConsentState.ALLOWED)

            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        davonGroup.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.DENIED,
                    ),
                ),
            )
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.DENIED,
            )
            assertEquals(davonGroup.consentState(), ConsentState.DENIED)

            davonGroup.updateConsentState(ConsentState.ALLOWED)
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.ALLOWED,
            )
            assertEquals(davonGroup.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun test5_CanAllowAndDenyInboxId() {
        runBlocking {
            val davonGroup =
                runBlocking {
                    davonSCWClient.conversations.newGroup(
                        listOf(boEOAClient.inboxId, eriSCWClient.inboxId),
                    )
                }
            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.UNKNOWN,
            )
            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        boEOAClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.ALLOWED,
                    ),
                ),
            )
            var alixMember = davonGroup.members().firstOrNull { it.inboxId == boEOAClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)

            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.ALLOWED,
            )

            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        boEOAClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.DENIED,
                    ),
                ),
            )
            alixMember = davonGroup.members().firstOrNull { it.inboxId == boEOAClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.DENIED)

            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.DENIED,
            )
        }
    }

    @Test
    fun test6_CanStreamAllMessages() =
        runBlocking {
            val group1 =
                davonSCWClient.conversations.newGroup(listOf(boEOAClient.inboxId, eriSCWClient.inboxId))
            val group2 =
                boEOAClient.conversations.newGroup(listOf(davonSCWClient.inboxId, eriSCWClient.inboxId))
            val dm1 = davonSCWClient.conversations.findOrCreateDm(eriSCWClient.inboxId)
            val dm2 = boEOAClient.conversations.findOrCreateDm(davonSCWClient.inboxId)
            davonSCWClient.conversations.sync()

            val retained = davonSCWClient.conversations.messageHistorySnapshot(10U).messages
            assertEquals(4, retained.size)
            assertEquals(setOf(group1.id, group2.id, dm1.id, dm2.id), retained.map { it.conversationId }.toSet())
            assertTrue(retained.all { it.kind == FfiConversationMessageKind.MEMBERSHIP_CHANGE })
            val messages = StreamTestMessages()
            val job =
                launch(Dispatchers.IO) {
                    davonSCWClient.conversations.streamAllMessages().collect { messages.add(it) }
                }
            try {
                messages.awaitHistory(retained)
                val expected = mutableListOf(group1.send("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(group2.send("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(dm1.send("hi") to "hi")
                messages.awaitApplications(expected)
                expected.add(dm2.send("hi") to "hi")
                messages.awaitApplications(expected)

                val history = davonSCWClient.conversations.messageHistorySnapshot(10U).messages
                assertEquals(retained.size + expected.size, history.size)
                assertEquals(
                    retained.map { it.id },
                    history.filter { it.kind == FfiConversationMessageKind.MEMBERSHIP_CHANGE }.map { it.id },
                )
                messages.awaitHistory(history)
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test
    fun test7_CanStreamConversations() =
        runBlocking {
            val conversations = mutableListOf<Pair<String, String>>()
            val ready = CompletableDeferred<Unit>()

            fun snapshot(): List<Pair<String, String>> = synchronized(conversations) { conversations.toList() }

            val job =
                launch(Dispatchers.IO) {
                    davonSCWClient.conversations
                        .streamWithReadiness(onReady = { ready.complete(Unit) })
                        .collect { conversation ->
                            synchronized(conversations) { conversations.add(conversation.id to conversation.topic) }
                        }
                }
            try {
                withTimeout(30_000) { ready.await() }
                val group1 =
                    eriSCWClient.conversations.newGroup(listOf(boEOAClient.inboxId, davonSCWClient.inboxId))
                val group2 =
                    boEOAClient.conversations.newGroup(listOf(eriSCWClient.inboxId, davonSCWClient.inboxId))
                val dm1 = davonSCWClient.conversations.findOrCreateDm(fixtures.alixClient.inboxId)
                val dm2 = fixtures.caroClient.conversations.findOrCreateDm(davonSCWClient.inboxId)
                val expected =
                    listOf(
                        group1.id to group1.topic,
                        group2.id to group2.topic,
                        dm1.id to dm1.topic,
                        dm2.id to dm2.topic,
                    )

                withTimeout(30_000) {
                    while (snapshot().size < expected.size) {
                        delay(10)
                    }
                }
                assertEquals(expected.sortedBy { it.first }, snapshot().sortedBy { it.first })
            } finally {
                withContext(NonCancellable) { job.cancelAndJoin() }
            }
        }

    @Test
    fun test8_AddAndRemovingAccounts() {
        val davonEOA = PrivateKeyBuilder()
        val davonSCW2 = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_3)

        runBlocking { davonSCWClient.addAccount(davonEOA) }
        runBlocking { davonSCWClient.addAccount(davonSCW2) }

        var state = runBlocking { davonSCWClient.inboxState(true) }
        assertEquals(state.installations.size, 1)
        assertEquals(state.identities.size, 3)
        assertEquals(state.recoveryPublicIdentity.identifier, davonSCW.publicIdentity.identifier)
        assertEquals(
            state.identities.map { it.identifier }.sorted(),
            listOf(
                davonEOA.publicIdentity.identifier,
                davonSCW2.publicIdentity.identifier,
                davonSCW.publicIdentity.identifier,
            ).sorted(),
        )

        runBlocking { davonSCWClient.removeAccount(davonSCW, davonSCW2.publicIdentity) }
        state = runBlocking { davonSCWClient.inboxState(true) }
        assertEquals(state.identities.size, 2)
        assertEquals(state.recoveryPublicIdentity.identifier, davonSCW.publicIdentity.identifier)
        assertEquals(
            state.identities.map { it.identifier }.sorted(),
            listOf(davonEOA.publicIdentity.identifier, davonSCW.publicIdentity.identifier)
                .sorted(),
        )
        assertEquals(state.installations.size, 1)

        // Cannot remove the recovery address
        Assert.assertThrows("Client error: Unknown Signer", FfiException::class.java) {
            runBlocking { davonSCWClient.removeAccount(davonEOA, davonSCW.publicIdentity) }
        }
    }
}
