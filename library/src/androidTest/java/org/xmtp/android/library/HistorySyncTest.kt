package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import java.io.File

@RunWith(AndroidJUnit4::class)
class HistorySyncTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var alixClient2: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var fixtures: Fixtures
    private lateinit var alixGroup: Group
    private lateinit var alix2Group: Group

    @Before
    fun setUp() {
        fixtures = fixtures()
        alixWallet = fixtures.alixAccount
        alix = fixtures.alix
        boWallet = fixtures.boAccount
        bo = fixtures.bo
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
        alixClient = fixtures.alixClient

        alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(bo.walletAddress)) }

        alixClient2 = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = fixtures.context,
                    dbEncryptionKey = fixtures.key,
                    dbDirectory = fixtures.context.filesDir.absolutePath.toString()
                )
            )
        }

        runBlocking {
            alixGroup.updateConsentState(ConsentState.DENIED)
            alixClient.conversations.syncAllConversations()
            alixClient2.conversations.syncAllConversations()
            alix2Group = alixClient2.findGroup(alixGroup.id)!!
        }
    }

    @Test
    fun testSyncConsent() {
        assertEquals(alixGroup.consentState(), ConsentState.DENIED)
        assertEquals(alix2Group.consentState(), ConsentState.UNKNOWN)
        val state = runBlocking { alixClient2.inboxState(true) }
        assertEquals(state.installations.size, 2)

        runBlocking {
            alixClient2.preferences.syncConsent()
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
            assertEquals(ConsentState.DENIED, alix2Group.consentState())
            alixClient2.preferences.setConsentState(
                listOf(
                    ConsentRecord(
                        alix2Group.id,
                        EntryType.CONVERSATION_ID,
                        ConsentState.ALLOWED
                    )
                )
            )
            assertEquals(
                alixClient2.preferences.conversationState(alix2Group.id),
                ConsentState.ALLOWED
            )
            assertEquals(alix2Group.consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testSyncMessages() {
        runBlocking {
            alixGroup.send("A message")
            alixGroup.send("A second message")
        }
        assertEquals(runBlocking { alixGroup.messages() }.size, 3)
        assertEquals(runBlocking { alix2Group.messages() }.size, 0)
        val state = runBlocking { alixClient2.inboxState(true) }
        assertEquals(state.installations.size, 2)

        val alixClient3 = runBlocking {
            Client().create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = fixtures.context,
                    dbEncryptionKey = fixtures.key,
                    dbDirectory = File(fixtures.context.filesDir.absolutePath, "xmtp_db3").toPath()
                        .toString()
                )
            )
        }

        val state3 = runBlocking { alixClient3.inboxState(true) }
        assertEquals(state3.installations.size, 3)

        runBlocking {
            alix2Group.send("A message")
            alix2Group.send("A second message")
            Thread.sleep(2000)
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient3.conversations.syncAllConversations()
            Thread.sleep(2000)

            val alix3Groups = alixClient3.conversations.listGroups()
            assertEquals(alix3Groups.size, 1)
            val alix3Group = alixClient3.findGroup(alixGroup.id)
                ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
            assertEquals(runBlocking { alixGroup.messages() }.size, 5)
            assertEquals(runBlocking { alix2Group.messages() }.size, 5)
            assertEquals(runBlocking { alix3Group.messages() }.size, 5)
        }
    }

    @Test
    fun testStreamConsent() {
        val consent = mutableListOf<ConsentRecord>()
        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.preferences.streamConsent()
                    .collect { entry ->
                        consent.add(entry)
                    }
            } catch (e: Exception) {
            }
        }

        Thread.sleep(2000)

        runBlocking {
            alix2Group.updateConsentState(ConsentState.DENIED)
            val dm3 = alixClient2.conversations.newConversation(caro.walletAddress)
            dm3.updateConsentState(ConsentState.DENIED)
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
        }

        Thread.sleep(2000)
        assertEquals(3, consent.size)
        assertEquals(alixGroup.consentState(), ConsentState.DENIED)
        job.cancel()
    }

    @Test
    fun testStreamPreferenceUpdates() {
        var preferences = 0
        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.preferences.streamPreferenceUpdates()
                    .collect { entry ->
                        preferences++
                    }
            } catch (e: Exception) {
            }
        }

        Thread.sleep(2000)

        runBlocking {
            val alixClient3 = runBlocking {
                Client().create(
                    account = alixWallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = fixtures.context,
                        dbEncryptionKey = fixtures.key,
                        dbDirectory = File(fixtures.context.filesDir.absolutePath, "xmtp_db3").toPath()
                            .toString()
                    )
                )
            }
            alixClient3.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
        }

        Thread.sleep(2000)
        assertEquals(2, preferences)
        job.cancel()
    }
}
