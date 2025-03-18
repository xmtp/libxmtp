package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.io.File

@RunWith(AndroidJUnit4::class)
class HistorySyncTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        fixtures = fixtures()
        alixWallet = fixtures.alixAccount
        alix = fixtures.alix
        boWallet = fixtures.boAccount
        bo = fixtures.bo
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testSyncConsent() = runBlocking {
        val alixGroup = alixClient.conversations.newGroup(listOf(fixtures.boClient.inboxId))
        val intialConsent = alixGroup.consentState()
        assertEquals(intialConsent, ConsentState.ALLOWED)

        val alixClient2 = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = fixtures.context,
                dbEncryptionKey = fixtures.key,
                dbDirectory = fixtures.context.filesDir.absolutePath.toString()
            )
        )

        val state = alixClient2.inboxState(true)
        assertEquals(state.installations.size, 2)

        alixClient.conversations.syncAllConversations()
        delay(2000)
        alixClient2.conversations.syncAllConversations()
        delay(2000)
        val alixGroup2 = alixClient2.conversations.findGroup(alixGroup.id)
            ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
        assertEquals(alixGroup2.consentState(), ConsentState.UNKNOWN)

        alixGroup.updateConsentState(ConsentState.DENIED)
        alixClient.preferences.syncConsent()
        delay(2000)
        alixClient.conversations.syncAllConversations()
        delay(2000)
        alixClient2.preferences.syncConsent()
        delay(2000)
        alixClient2.conversations.syncAllConversations()
        delay(2000)

        assertEquals(alixGroup2.consentState(), ConsentState.DENIED)
    }

    @Test
    fun testSyncMessages() = runBlocking {
        val alixGroup = alixClient.conversations.newGroup(listOf(fixtures.boClient.inboxId))
        val initialMessageCount = alixGroup.messages().size
        assertEquals(initialMessageCount, 1)

        val alixClient2 = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = fixtures.context,
                dbEncryptionKey = fixtures.key,
                dbDirectory = fixtures.context.filesDir.absolutePath.toString()
            )
        )

        val state = alixClient2.inboxState(true)
        assertEquals(state.installations.size, 2)

        alixGroup.send("hi")

        // Sync all conversations
        alixClient.conversations.syncAllConversations()
        delay(2000)
        alixClient2.conversations.syncAllConversations()
        delay(2000)

        val alixGroup2 = alixClient2.conversations.findGroup(alixGroup.id)
            ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")

        val messageCount2 = alixGroup2.messages().size
        assertEquals(messageCount2, 1)
    }

    @Test
    fun testStreamConsent() {
        val alixClient2 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = fixtures.context,
                    dbEncryptionKey = fixtures.key,
                    dbDirectory = fixtures.context.filesDir.absolutePath.toString()
                )
            )
        }
        val alixGroup = runBlocking { alixClient.conversations.newGroup(listOf(boClient.inboxId)) }
        runBlocking {
            alixGroup.updateConsentState(ConsentState.DENIED)
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
        }
        val alix2Group = alixClient2.conversations.findGroup(alixGroup.id)!!

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
            val dm3 = alixClient2.conversations.newConversation(caroClient.inboxId)
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
        val alixClient2 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = fixtures.context,
                    dbEncryptionKey = fixtures.key,
                    dbDirectory = fixtures.context.filesDir.absolutePath.toString()
                )
            )
        }
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
                Client.create(
                    account = alixWallet,
                    options = ClientOptions(
                        ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                        appContext = fixtures.context,
                        dbEncryptionKey = fixtures.key,
                        dbDirectory = File(
                            fixtures.context.filesDir.absolutePath,
                            "xmtp_db3"
                        ).toPath()
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
