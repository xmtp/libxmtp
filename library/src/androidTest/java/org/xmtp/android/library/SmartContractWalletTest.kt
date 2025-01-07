package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.BeforeClass
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.GenericException

@RunWith(AndroidJUnit4::class)
class SmartContractWalletTest {
    companion object {
        private lateinit var davonSCW: FakeSCWWallet
        private lateinit var davonSCWClient: Client
        private lateinit var eriSCW: FakeSCWWallet
        private lateinit var eriSCWClient: Client
        private lateinit var options: ClientOptions
        private lateinit var boEOAWallet: PrivateKeyBuilder
        private lateinit var boEOA: PrivateKey
        private lateinit var boEOAClient: Client

        @BeforeClass
        @JvmStatic
        fun setUpClass() {
            val key = byteArrayOf(
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
                0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
                0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F
            )
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = context,
                dbEncryptionKey = key
            )

            // EOA
            boEOAWallet = PrivateKeyBuilder()
            boEOA = boEOAWallet.getPrivateKey()
            boEOAClient = runBlocking {
                Client().create(
                    account = boEOAWallet,
                    options = options
                )
            }

            // SCW
            davonSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_1)
            davonSCWClient = runBlocking {
                Client().create(
                    account = davonSCW,
                    options = options
                )
            }

            // SCW
            eriSCW = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_2)
            eriSCWClient = runBlocking {
                Client().create(
                    account = eriSCW,
                    options = options
                )
            }
        }
    }

    @Test
    fun testCanBuildASCW() {
        val davonSCWClient2 = runBlocking {
            Client().build(
                address = davonSCW.address,
                options = options
            )
        }

        assertEquals(davonSCWClient.inboxId, davonSCWClient2.inboxId)
    }

    @Test
    fun testAddAndRemovingAccounts() {
        val davonEOA = PrivateKeyBuilder()
        val davonSCW2 = FakeSCWWallet.generate(ANVIL_TEST_PRIVATE_KEY_3)

        runBlocking { davonSCWClient.addAccount(davonEOA) }
        runBlocking { davonSCWClient.addAccount(davonSCW2) }

        var state = runBlocking { davonSCWClient.inboxState(true) }
        assertEquals(state.installations.size, 1)
        assertEquals(state.addresses.size, 3)
        assertEquals(state.recoveryAddress, davonSCWClient.address.lowercase())
        assertEquals(
            state.addresses.sorted(),
            listOf(
                davonEOA.address.lowercase(),
                davonSCW2.address.lowercase(),
                davonSCWClient.address.lowercase()
            ).sorted()
        )

        runBlocking { davonSCWClient.removeAccount(davonSCW, davonSCW2.address) }
        state = runBlocking { davonSCWClient.inboxState(true) }
        assertEquals(state.addresses.size, 2)
        assertEquals(state.recoveryAddress, davonSCWClient.address.lowercase())
        assertEquals(
            state.addresses.sorted(),
            listOf(
                davonEOA.address.lowercase(),
                davonSCWClient.address.lowercase()
            ).sorted()
        )
        assertEquals(state.installations.size, 1)

        // Cannot remove the recovery address
        Assert.assertThrows(
            "Client error: Unknown Signer",
            GenericException::class.java
        ) {
            runBlocking {
                davonSCWClient.removeAccount(
                    davonEOA,
                    davonSCWClient.address
                )
            }
        }
    }

    @Test
    fun testsCanCreateGroup() {
        val group1 = runBlocking {
            boEOAClient.conversations.newGroup(
                listOf(
                    davonSCW.walletAddress,
                    eriSCW.walletAddress
                )
            )
        }
        val group2 = runBlocking {
            davonSCWClient.conversations.newGroup(
                listOf(
                    boEOA.walletAddress,
                    eriSCW.walletAddress
                )
            )
        }

        assertEquals(
            runBlocking { group1.members().map { it.inboxId }.sorted() },
            listOf(davonSCWClient.inboxId, boEOAClient.inboxId, eriSCWClient.inboxId).sorted()
        )
        assertEquals(
            runBlocking { group2.members().map { it.addresses.first() }.sorted() },
            listOf(davonSCWClient.address, boEOAClient.address, eriSCWClient.address).sorted()
        )
    }

    @Test
    fun testsCanSendMessages() {
        val boGroup = runBlocking {
            boEOAClient.conversations.newGroup(
                listOf(
                    davonSCW.walletAddress,
                    eriSCW.walletAddress
                )
            )
        }
        runBlocking { boGroup.send("howdy") }
        val messageId = runBlocking { boGroup.send("gm") }
        runBlocking { boGroup.sync() }
        assertEquals(runBlocking { boGroup.messages() }.first().body, "gm")
        assertEquals(runBlocking { boGroup.messages() }.first().id, messageId)
        assertEquals(
            runBlocking { boGroup.messages() }.first().deliveryStatus,
            Message.MessageDeliveryStatus.PUBLISHED
        )
        assertEquals(runBlocking { boGroup.messages() }.size, 3)

        runBlocking { davonSCWClient.conversations.sync() }
        val davonGroup = runBlocking { davonSCWClient.conversations.listGroups().last() }
        runBlocking { davonGroup.sync() }
        assertEquals(runBlocking { davonGroup.messages() }.size, 2)
        assertEquals(runBlocking { davonGroup.messages() }.first().body, "gm")
        runBlocking { davonGroup.send("from davon") }

        runBlocking { eriSCWClient.conversations.sync() }
        val eriGroup = runBlocking { davonSCWClient.findGroup(davonGroup.id) }
        runBlocking { eriGroup?.sync() }
        assertEquals(runBlocking { eriGroup?.messages() }?.size, 3)
        assertEquals(runBlocking { eriGroup?.messages() }?.first()?.body, "from davon")
        runBlocking { eriGroup?.send("from eri") }
    }

    @Test
    fun testGroupConsent() {
        runBlocking {
            val davonGroup = runBlocking {
                davonSCWClient.conversations.newGroup(
                    listOf(
                        boEOA.walletAddress,
                        eriSCW.walletAddress
                    )
                )
            }
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.ALLOWED
            )
            assertEquals(davonGroup.consentState(), ConsentState.ALLOWED)

            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        davonGroup.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.DENIED
                    )
                )
            )
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.DENIED
            )
            assertEquals(davonGroup.consentState(), ConsentState.DENIED)

            davonGroup.updateConsentState(ConsentState.ALLOWED)
            assertEquals(
                davonSCWClient.preferences.conversationState(davonGroup.id),
                ConsentState.ALLOWED
            )
            assertEquals(davonGroup.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testCanAllowAndDenyInboxId() {
        runBlocking {
            val davonGroup = runBlocking {
                davonSCWClient.conversations.newGroup(
                    listOf(
                        boEOA.walletAddress,
                        eriSCW.walletAddress
                    )
                )
            }
            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.UNKNOWN
            )
            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        boEOAClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.ALLOWED
                    )
                )
            )
            var alixMember = davonGroup.members().firstOrNull { it.inboxId == boEOAClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)

            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.ALLOWED
            )

            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        boEOAClient.inboxId,
                        EntryType.INBOX_ID,
                        ConsentState.DENIED
                    )
                )
            )
            alixMember = davonGroup.members().firstOrNull { it.inboxId == boEOAClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.DENIED)

            assertEquals(
                davonSCWClient.preferences.inboxIdState(boEOAClient.inboxId),
                ConsentState.DENIED
            )

            davonSCWClient.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        eriSCWClient.address,
                        EntryType.ADDRESS,
                        ConsentState.ALLOWED
                    )
                )
            )
            alixMember = davonGroup.members().firstOrNull { it.inboxId == eriSCWClient.inboxId }
            assertEquals(alixMember!!.consentState, ConsentState.ALLOWED)
            assertEquals(
                davonSCWClient.preferences.inboxIdState(eriSCWClient.inboxId),
                ConsentState.ALLOWED
            )
            assertEquals(
                davonSCWClient.preferences.addressState(eriSCWClient.address),
                ConsentState.ALLOWED
            )
        }
    }

    @Test
    fun testCanStreamAllMessages() {
        val group1 = runBlocking {
            davonSCWClient.conversations.newGroup(
                listOf(
                    boEOA.walletAddress,
                    eriSCW.walletAddress
                )
            )
        }
        val group2 = runBlocking {
            boEOAClient.conversations.newGroup(
                listOf(
                    davonSCW.walletAddress,
                    eriSCW.walletAddress
                )
            )
        }
        val dm1 = runBlocking { davonSCWClient.conversations.findOrCreateDm(eriSCW.walletAddress) }
        val dm2 = runBlocking { boEOAClient.conversations.findOrCreateDm(davonSCW.walletAddress) }
        runBlocking { davonSCWClient.conversations.sync() }

        val allMessages = mutableListOf<Message>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                davonSCWClient.conversations.streamAllMessages()
                    .collect { message ->
                        allMessages.add(message)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)
        runBlocking {
            group1.send("hi")
            group2.send("hi")
            dm1.send("hi")
            dm2.send("hi")
        }
        Thread.sleep(1000)
        assertEquals(4, allMessages.size)
        job.cancel()
    }

    @Test
    fun testCanStreamConversations() {
        val allMessages = mutableListOf<String>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                davonSCWClient.conversations.stream()
                    .collect { message ->
                        allMessages.add(message.topic)
                    }
            } catch (e: Exception) {
            }
        }
        Thread.sleep(1000)

        runBlocking {
            eriSCWClient.conversations.newGroup(listOf(boEOA.walletAddress, davonSCW.walletAddress))
            boEOAClient.conversations.newGroup(listOf(eriSCW.walletAddress, davonSCW.walletAddress))
            eriSCWClient.conversations.findOrCreateDm(davonSCW.walletAddress)
            boEOAClient.conversations.findOrCreateDm(davonSCW.walletAddress)
        }

        Thread.sleep(1000)
        assertEquals(4, allMessages.size)
        job.cancel()
    }
}
