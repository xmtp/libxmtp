package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
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
import java.security.SecureRandom

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
        val initialConsent = alixGroup.consentState()
        assertEquals(initialConsent, ConsentState.ALLOWED)

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
        alixClient2.conversations.syncAllConversations()

        val alixGroup2 = alixClient2.conversations.findGroup(alixGroup.id)
            ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
        assertEquals(alixGroup2.consentState(), ConsentState.UNKNOWN)

        alixGroup.updateConsentState(ConsentState.DENIED)
        alixClient.preferences.sync()
        alixGroup.sync()

        alixClient2.preferences.sync()
        delay(2000)
        alixGroup2.sync()

        assertEquals(alixGroup2.consentState(), ConsentState.DENIED)
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
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
        }
        val alix2Group = runBlocking { alixClient2.conversations.findGroup(alixGroup.id)!! }

        val consent = mutableListOf<ConsentRecord>()
        val job1 = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages()
                    .collect { }
            } catch (e: Exception) {
            }
        }
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
        assertEquals(5, consent.size)
        assertEquals(runBlocking { alixGroup.consentState() }, ConsentState.DENIED)
        job.cancel()
        job1.cancel()
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
                    dbDirectory = fixtures.context.filesDir.absolutePath.toString(),
                    historySyncUrl = null
                )
            )
        }
        var preferences = 0
        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient2.preferences.streamPreferenceUpdates()
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
                        historySyncUrl = null,
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
            alixClient2.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient.conversations.syncAllConversations()
            Thread.sleep(2000)
        }

        Thread.sleep(2000)
        assertEquals(1, preferences)
        job.cancel()
    }

    @Test
    fun testDisablingHistoryTransferStillSyncsLocalState() = runBlocking {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            appContext = context,
            dbEncryptionKey = key,
            historySyncUrl = null
        )
        val alix1 =
            Client.create(
                account = fakeWallet,
                options = options
            )

        val alix2 =
            Client.create(
                account = fakeWallet,
                options = options
            )

        val alixGroup = runBlocking { alix1.conversations.newGroup(listOf(boClient.inboxId)) }

        alix1.conversations.syncAllConversations()
        alix2.conversations.syncAllConversations()

        val alixGroup2 = alix2.conversations.findGroup(alixGroup.id)
            ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
        assertEquals(alixGroup2.consentState(), ConsentState.ALLOWED)

        alixGroup.updateConsentState(ConsentState.DENIED)
        alix1.preferences.sync()
        alixGroup.sync()

        alix2.preferences.sync()
        delay(2000)
        alixGroup2.sync()

        // Validate the updated consent is visible on second client
        assertEquals(alixGroup2.consentState(), ConsentState.DENIED)
    }

    @Test
    fun testDisablingHistoryTransferDoesNotTransfer() = runBlocking {
        val alixGroup = alixClient.conversations.newGroup(listOf(fixtures.boClient.inboxId))
        val initialMessageCount = alixGroup.messages().size
        assertEquals(initialMessageCount, 1)
        val alixClient2 = Client.create(
            account = alixWallet,
            options = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                appContext = fixtures.context,
                dbEncryptionKey = fixtures.key,
                historySyncUrl = null,
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
        alixClient.preferences.sync()
        delay(2000)
        alixClient2.preferences.sync()
        delay(2000)

        val alixGroup2 = alixClient2.conversations.findGroup(alixGroup.id)
            ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")

        alixGroup2.sync()

        val messageCount2 = alixGroup2.messages().size
        assertEquals(messageCount2, 2)
    }
}
