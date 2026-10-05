package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class HistorySyncTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: SDKClient
    private lateinit var boClient: SDKClient
    private lateinit var alixWallet: Signer

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        alixWallet = fixtures.alixAccount
    }

    private suspend fun waitUntil(condition: suspend () -> Boolean) =
        withTimeout(30_000) {
            while (!condition()) delay(100)
        }

    private suspend fun sync(vararg clients: SDKClient) {
        clients.forEach {
            it.conversations().syncAll(null)
            it.preferences().sync()
        }
    }

    private suspend fun copiedGroup(
        id: ConversationId,
        vararg clients: SDKClient,
    ): Group {
        var copied: Group? = null
        waitUntil {
            sync(alixClient, *clients)
            copied =
                clients
                    .first()
                    .conversations()
                    .listGroups(null)
                    .find { it.id() == id }
            copied != null
        }
        return checkNotNull(copied)
    }

    @Test fun testSyncConsent() =
        runBlocking {
            val boGroup = boClient.conversations().createGroup(listOf(alixClient.inboxId()))
            alixClient.conversations().sync()
            val alixGroup = alixClient.conversations().listGroups(null).single()
            assertEquals(ConsentState.UNKNOWN, alixGroup.state().common.consentState)
            val second = createClient(alixWallet)
            assertEquals(2, second.inboxState(true).installations.size)
            println("SYNC_CONSENT stage=copy-start")
            val copy = copiedGroup(boGroup.id(), second)
            println("SYNC_CONSENT stage=copy-ready consent=${copy.state().common.consentState}")
            assertEquals(ConsentState.UNKNOWN, copy.state().common.consentState)
            alixGroup.updateConsentState(ConsentState.DENIED)
            println("SYNC_CONSENT stage=update source=${alixGroup.state().common.consentState}")
            var observation = 0
            waitUntil {
                sync(alixClient, second)
                val state = copy.state().common.consentState
                if (observation++ % 20 == 0) println("SYNC_CONSENT stage=propagate attempt=$observation consent=$state")
                state == ConsentState.DENIED
            }
            assertEquals(ConsentState.DENIED, copy.state().common.consentState)
        }

    @Test fun testStreamConsent() =
        runBlocking {
            val second = createClient(alixWallet)
            val original = alixClient.conversations().createGroup(listOf(boClient.inboxId()))
            val copy = copiedGroup(original.id(), second)
            val changed = CompletableDeferred<ClientEvent.ConsentChanged>()
            val listener =
                alixClient.startListener(
                    EventFilter(listOf(EventKind.CONSENT_CHANGED), null, null, false),
                    { event ->
                        if (event is ClientEvent.ConsentChanged && event.consentChanged.entity == original.id() &&
                            event.consentChanged.state == EventConsentState.DENIED
                        ) {
                            changed.complete(event)
                        }
                    },
                )
            try {
                copy.updateConsentState(ConsentState.DENIED)
                waitUntil {
                    sync(second, alixClient)
                    changed.isCompleted
                }
                assertEquals(original.id(), changed.await().consentChanged.entity)
                assertEquals(EventConsentState.DENIED, changed.await().consentChanged.state)
                assertEquals(ConsentState.DENIED, original.state().common.consentState)
            } finally {
                withContext(NonCancellable) { alixClient.stopListener(listener) }
            }
        }

    @Test fun testStreamPreferenceUpdates() =
        runBlocking {
            val second = createClient(alixWallet)
            val updated = CompletableDeferred<Unit>()
            val listener =
                second.startListener(
                    EventFilter(listOf(EventKind.HMAC_KEYS_UPDATED), null, null, false),
                    { event -> if (event is ClientEvent.HmacKeysUpdated) updated.complete(Unit) },
                )
            try {
                val third = createClient(alixWallet)
                waitUntil {
                    sync(alixClient, second, third)
                    updated.isCompleted
                }
                assertTrue(updated.isCompleted)
                assertEquals(3, second.inboxState(true).installations.size)
            } finally {
                withContext(NonCancellable) { second.stopListener(listener) }
            }
        }

    @Test fun testV3CanMessageV3() =
        runBlocking {
            val wallet = createWallet()
            val first = createClient(wallet)
            val second = createClient(wallet)
            val third = createClient(wallet)
            val original = first.conversations().createGroup(listOf(boClient.inboxId()))
            var copy: Group? = null
            waitUntil {
                sync(first, second, third)
                copy = second.conversations().listGroups(null).find { it.id() == original.id() }
                copy != null
            }
            val copied = checkNotNull(copy)
            waitUntil {
                sync(first, second)
                copied.state().common.consentState == ConsentState.ALLOWED
            }
            assertEquals(ConsentState.ALLOWED, copied.state().common.consentState)
            original.updateConsentState(ConsentState.DENIED)
            waitUntil {
                sync(first, second)
                copied.state().common.consentState == ConsentState.DENIED
            }
            assertEquals(ConsentState.DENIED, copied.state().common.consentState)
        }
}
