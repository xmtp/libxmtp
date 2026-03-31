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

    private suspend fun waitUntil(
        timeoutMs: Long = 30_000,
        intervalMs: Long = 500,
        condition: suspend () -> Boolean,
    ) {
        val start = System.currentTimeMillis()
        while (System.currentTimeMillis() - start < timeoutMs) {
            if (condition()) return
            delay(intervalMs)
        }
    }

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

            // Sync both installations until client2 can find the group
            var alixGroup2: Group? = null
            waitUntil {
                alixClient.conversations.syncAllConversations()
                alixClient2.conversations.syncAllConversations()
                alixClient.preferences.sync()
                alixClient2.preferences.sync()
                alixGroup2 = alixClient2.conversations.findGroup(alixGroup.id)
                alixGroup2 != null
            }
            val group2 =
                alixGroup2
                    ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
            assertEquals(group2.consentState(), ConsentState.UNKNOWN)

            alixGroup.updateConsentState(ConsentState.DENIED)

            // Sync both clients until consent propagates to client2.
            // Client1 publishes the worker-queued intent, client2 pulls the update.
            waitUntil {
                alixClient.preferences.sync()
                alixClient2.preferences.sync()
                group2.consentState() == ConsentState.DENIED
            }

            assertEquals(group2.consentState(), ConsentState.DENIED)
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

    @Test
    fun testV3CanMessageV3() =
        runBlocking {
            val wallet = createWallet()
            val client1 = createClient(wallet)
            val client2 = createClient(wallet)
            val client3 = createClient(wallet)

            val group = client1.conversations.newGroup(listOf(boClient.inboxId))

            // Sync all installations until client2 can find the group
            var client2Group: Group? = null
            waitUntil {
                client1.conversations.syncAllConversations()
                client2.conversations.syncAllConversations()
                client3.conversations.syncAllConversations()
                client1.preferences.sync()
                client2.preferences.sync()
                client3.preferences.sync()
                client2Group = client2.conversations.findGroup(group.id)
                client2Group != null
            }
            val c2Group =
                client2Group
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")

            // Wait for client2 to see the ALLOWED consent state from client1
            waitUntil {
                client1.preferences.sync()
                client2.preferences.sync()
                c2Group.consentState() == ConsentState.ALLOWED
            }
            assertEquals(ConsentState.ALLOWED, c2Group.consentState())

            group.updateConsentState(ConsentState.DENIED)

            // Wait for consent change to propagate to client2
            waitUntil {
                client1.preferences.sync()
                client2.preferences.sync()
                c2Group.consentState() == ConsentState.DENIED
            }

            assertEquals(ConsentState.DENIED, c2Group.consentState())
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
