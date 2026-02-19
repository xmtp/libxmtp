package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder

@RunWith(AndroidJUnit4::class)
class HistorySyncTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client
    private lateinit var caroClient: Client
    private lateinit var alixWallet: PrivateKeyBuilder

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
        alixWallet = fixtures.alixAccount
    }

    @Ignore("Flaky: consent sync timing is non-deterministic")
    @Test
    fun testSyncConsent() =
        runBlocking {
            val boGroup = boClient.conversations.newGroup(listOf(alixClient.inboxId))
            alixClient.conversations.sync()

            val alixGroup =
                alixClient.conversations.findGroup(boGroup.id)
                    ?: throw AssertionError("Failed to find group with ID: ${boGroup.id}")
            val initialConsent = alixGroup.consentState()
            assertEquals(initialConsent, ConsentState.UNKNOWN)

            val alixClient2 = createClient(alixWallet)

            val state = alixClient2.inboxState(true)
            assertEquals(state.installations.size, 2)

            // Sync all of the first client's conversations to add Alix2
            alixGroup.sync()

            alixClient2.conversations.sync()

            val alixGroup2 =
                alixClient2.conversations.findGroup(alixGroup.id)
                    ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
            assertEquals(alixGroup2.consentState(), ConsentState.UNKNOWN)

            alixGroup.updateConsentState(ConsentState.DENIED)
            alixClient.preferences.sync()
            delay(1000)
            alixClient2.preferences.sync()
            delay(4000)

            assertEquals(alixGroup2.consentState(), ConsentState.DENIED)
        }

    @Test
    fun testStreamConsent() {
        val alixClient2 =
            runBlocking {
                createClient(alixWallet)
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
        val job1 =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    alixClient.conversations.streamAllMessages().collect {}
                } catch (e: Exception) {
                }
            }
        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    alixClient.preferences.streamConsent().collect { entry ->
                        consent.add(entry)
                    }
                } catch (e: Exception) {
                }
            }

        Thread.sleep(2000)

        runBlocking {
            alix2Group.updateConsentState(ConsentState.DENIED)
            alixClient2.preferences.sync()
            Thread.sleep(2000)
        }

        Thread.sleep(2000)
        assertEquals(1, consent.size)
        assertEquals(runBlocking { alixGroup.consentState() }, ConsentState.DENIED)
        job.cancel()
        job1.cancel()
    }

    @Test
    fun testStreamPreferenceUpdates() {
        val alixClient2 =
            runBlocking {
                createClient(alixWallet)
            }
        var preferences = 0
        val job =
            CoroutineScope(Dispatchers.IO).launch {
                try {
                    alixClient2.preferences.streamPreferenceUpdates().collect { entry ->
                        preferences++
                    }
                } catch (e: Exception) {
                }
            }

        Thread.sleep(2000)

        runBlocking {
            val alixClient3 =
                runBlocking {
                    createClient(alixWallet)
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

    @Ignore("Flaky: consent sync timing is non-deterministic")
    @Test
    fun testV3CanMessageV3() =
        runBlocking {
            val wallet = createWallet()
            val client1 = createClient(wallet)
            val client2 = createClient(wallet)
            val client3 = createClient(wallet)

            val group = client1.conversations.newGroup(listOf(boClient.inboxId))

            client2.conversations.sync()
            client3.conversations.sync()

            val client2Group =
                client2.conversations.findGroup(group.id)
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")
            assertEquals(ConsentState.ALLOWED, client2Group.consentState())

            group.updateConsentState(ConsentState.DENIED)
            client1.preferences.sync()

            // Poll until client2 sees the consent change or timeout
            val timeout = 10_000L
            val interval = 500L
            var elapsed = 0L
            while (elapsed < timeout) {
                delay(interval)
                elapsed += interval
                client2.preferences.sync()
                if (client2Group.consentState() == ConsentState.DENIED) break
            }

            assertEquals(ConsentState.DENIED, client2Group.consentState())
        }

    @Test
    fun testDisablingHistoryTransferDoesNotTransfer() =
        runBlocking {
            val alixGroup = alixClient.conversations.newGroup(listOf(boClient.inboxId))
            val initialMessageCount = alixGroup.messages().size
            assertEquals(initialMessageCount, 1)
            val alixClient2 =
                Client.create(
                    account = alixWallet,
                    options =
                        ClientOptions(
                            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                            appContext = context,
                            dbEncryptionKey = dbEncryptionKey,
                            dbDirectory = context.filesDir.absolutePath.toString(),
                        ),
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

            val alixGroup2 =
                alixClient2.conversations.findGroup(alixGroup.id)
                    ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")

            alixGroup2.sync()

            val messageCount2 = alixGroup2.messages().size
            assertEquals(messageCount2, 2)
        }
}
