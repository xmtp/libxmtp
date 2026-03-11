package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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
            alixClient2.preferences.sync()

            // Poll until alixClient2 observes the denied consent state.
            val timeoutMs = 15_000L
            val intervalMs = 500L
            var elapsedMs = 0L
            var latestConsent = alixGroup2.consentState()

            while (elapsedMs < timeoutMs) {
                alixClient2.sendSyncRequest()
                alixClient.preferences.sync()
                alixClient2.preferences.sync()
                alixClient2.conversations.sync()

                val refreshedGroup2 =
                    alixClient2.conversations.findGroup(alixGroup.id)
                        ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")
                refreshedGroup2.sync()
                latestConsent = refreshedGroup2.consentState()
                if (latestConsent == ConsentState.DENIED) break

                delay(intervalMs)
                elapsedMs += intervalMs
            }

            assertEquals(ConsentState.DENIED, latestConsent)
        }

    @Test
    fun testSyncMessages() =
        runBlocking {
            val wallet = createWallet()
            val client1 = createClient(wallet)

            val group = client1.conversations.newGroup(listOf(boClient.inboxId))

            // Send a message before second installation is created
            val msgId = group.send("hi")
            val messageCount = group.messages().size
            assertEquals(messageCount, 2)

            val client2 = createClient(wallet)
            val state = client2.inboxState(true)
            assertEquals(state.installations.size, 2)

            client2.sendSyncRequest()

            client1.syncAllDeviceSyncGroups()
            delay(1000)
            client2.syncAllDeviceSyncGroups()
            delay(1000)

            val client1MessageCount = group.messages().size
            val group2 =
                client2.conversations.findGroup(group.id)
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")

            val messages = group2.messages()
            val containsMessage = messages.any { it.id == msgId }
            val client2MessageCount = messages.size
            assertTrue(containsMessage)
            assertEquals(client1MessageCount, client2MessageCount)
        }

    @Test
    fun testSyncDeviceArchive() =
        runBlocking {
            val wallet = createWallet()
            val client1 = createClient(wallet)

            val group = client1.conversations.newGroup(listOf(boClient.inboxId))
            val msgFromAlix = group.send("hello from alix")

            delay(1000)
            val client2 = createClient(wallet)
            delay(1000)

            client1.syncAllDeviceSyncGroups()
            client1.sendSyncArchive(pin = "123")
            delay(1000)

            boClient.conversations.syncAllConversations()
            val boGroup =
                boClient.conversations.findGroup(group.id)
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")
            boGroup.send("hello from bo")

            client1.conversations.syncAllConversations()
            client2.conversations.syncAllConversations()

            val group2Before =
                client2.conversations.findGroup(group.id)
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")
            val messagesBefore = group2Before.messages()
            assertEquals(messagesBefore.size, 2)

            delay(1000)
            client1.syncAllDeviceSyncGroups()
            delay(1000)
            client2.syncAllDeviceSyncGroups()

            // Mirrors current Swift test flow where archive listing is observed but not asserted.
            client2.listAvailableArchives(daysCutoff = 7)

            client2.processSyncArchive("123")
            client2.conversations.syncAllConversations()

            val group2After =
                client2.conversations.findGroup(group.id)
                    ?: throw AssertionError("Failed to find group with ID: ${group.id}")
            val messagesAfter = group2After.messages()
            assertEquals(messagesAfter.size, 3)
            assertTrue(messagesAfter.any { it.id == msgFromAlix })
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
            val localApi = ClientOptions.Api(XMTPEnvironment.LOCAL, false)
            val alixClient =
                Client.create(
                    account = alixWallet,
                    options = createClientOptions(api = localApi, deviceSyncEnabled = false),
                )
            val alixGroup = alixClient.conversations.newGroup(listOf(boClient.inboxId))
            val messageIdNotExpectedOnClient2 = alixGroup.send("hi")
            delay(2000)
            val initialMessageCount = alixGroup.messages().size
            assertEquals(initialMessageCount, 2)
            val alixClient2 =
                Client.create(
                    account = alixWallet,
                    options = createClientOptions(api = localApi, deviceSyncEnabled = true),
                )

            val state = alixClient2.inboxState(true)
            assertEquals(state.installations.size, 3)

            // This sync request will not be obeyed because device sync is disabled
            alixClient2.sendSyncRequest()

            // Sync all conversations
            delay(6000)
            alixGroup.send("this message will add alix2 to the group")
            alixClient.conversations.syncAllConversations()
            delay(1000)
            alixClient2.conversations.syncAllConversations()
            delay(1000)

            val alixGroup2 =
                alixClient2.conversations.findGroup(alixGroup.id)
                    ?: throw AssertionError("Failed to find group with ID: ${alixGroup.id}")

            alixGroup2.sync()

            val messages2 = alixGroup2.messages()
            assertFalse(
                messages2.any { it.id == messageIdNotExpectedOnClient2 },
            )
            assertEquals(messages2.size, initialMessageCount)
        }
}
