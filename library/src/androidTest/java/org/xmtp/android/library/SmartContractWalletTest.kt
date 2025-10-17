package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import uniffi.xmtpv3.GenericException
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
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
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
    fun test6_CanStreamAllMessages() {
        val group1 =
            runBlocking {
                davonSCWClient.conversations.newGroup(listOf(boEOAClient.inboxId, eriSCWClient.inboxId))
            }
        val group2 =
            runBlocking {
                boEOAClient.conversations.newGroup(listOf(davonSCWClient.inboxId, eriSCWClient.inboxId))
            }
        val dm1 = runBlocking { davonSCWClient.conversations.findOrCreateDm(eriSCWClient.inboxId) }
        val dm2 = runBlocking { boEOAClient.conversations.findOrCreateDm(davonSCWClient.inboxId) }
        runBlocking { davonSCWClient.conversations.sync() }

        val allMessages = mutableListOf<DecodedMessage>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                davonSCWClient.conversations.streamAllMessages().collect { message ->
                    allMessages.add(message)
                }
            }
        Thread.sleep(2000)
        runBlocking {
            group1.send("hi")
            group2.send("hi")
            dm1.send("hi")
            dm2.send("hi")
        }
        Thread.sleep(2000)
        assertEquals(4, allMessages.size)
        job.cancel()
    }

    @Test
    fun test7_CanStreamConversations() {
        val allMessages = mutableListOf<String>()

        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    davonSCWClient.conversations.stream().collect { message ->
                        allMessages.add(message.topic)
                    }
                } catch (e: Exception) {
                }
            }
        Thread.sleep(1000)

        runBlocking {
            eriSCWClient.conversations.newGroup(listOf(boEOAClient.inboxId, davonSCWClient.inboxId))
            boEOAClient.conversations.newGroup(listOf(eriSCWClient.inboxId, davonSCWClient.inboxId))
            davonSCWClient.conversations.findOrCreateDm(fixtures.alixClient.inboxId)
            fixtures.caroClient.conversations.findOrCreateDm(davonSCWClient.inboxId)
        }

        Thread.sleep(1000)
        assertEquals(4, allMessages.size)
        job.cancel()
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
        Assert.assertThrows("Client error: Unknown Signer", GenericException::class.java) {
            runBlocking { davonSCWClient.removeAccount(davonEOA, davonSCW.publicIdentity) }
        }
    }
}
