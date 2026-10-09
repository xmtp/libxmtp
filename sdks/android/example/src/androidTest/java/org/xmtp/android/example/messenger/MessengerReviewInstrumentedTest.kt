package org.xmtp.android.example.messenger

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.io.File
import java.nio.file.Files
import java.security.SecureRandom
import java.util.UUID

class MessengerReviewInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (
            error: TimeoutCancellationException,
        ) {
            throw AssertionError("Stage did not finish: $stage", error)
        }
    }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
        val owner = checkNotNull(model.session.active.value)
        until("connected UI") { model.state.value.inbox == owner.client.inboxId() }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.session.preferences.beforePositionCommit = {}
            model.session.preferences.positionCommitFinished = { _, _ -> }
            model.writeConsent = { chat, value -> chat.updateConsentState(value) }
            model.onConsentFinished = {}
            model.session.onSessionInvalidated = {}
            model.session.unregisterNotifications = {}
            if (model.session.active.value != null ||
                model.session.preferences.reset() != null
            ) {
                model.session.deleteAccount()
            }
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun lateBlockedConsentCannotReplaceNewChatSettings() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Blocked A"))
                val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Current B"))
                model.dispatch(MessengerAction.OpenConversation(a.id()))
                until("open A") { model.state.value.conversationId == a.id() }
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Unit>()
                model.writeConsent = { chat, value ->
                    entered.complete(Unit)
                    release.await()
                    chat.updateConsentState(value)
                }
                model.onConsentFinished = { finished.complete(Unit) }
                model.dispatch(MessengerAction.Consent(false))
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.OpenConversation(b.id()))
                until("open B") { model.state.value.conversationId == b.id() }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                until("settings B") { model.state.value.settings.title == "Current B" }
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                println("CONSENT_PROOF stage=old-sdk-write-completed screen=${model.state.value.screen}")
                assertEquals(ConsentState.DENIED, a.state().common.consentState)
                assertEquals(b.id(), model.state.value.conversationId)
                assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun failedResetOffersRealUiRetryWithoutAnOwnerAndKeepsPeerProfile() =
        runBlocking {
            var link: File? = null
            var peerRoot: File? = null
            var peerId: String? = null
            try {
                val owner = connect()
                val peer = BackendProfile(UUID.randomUUID().toString(), "http://peer.invalid")
                peerId = peer.id
                model.session.preferences.saveProfile(peer)
                peerRoot = peer.paths(compose.activity.filesDir).root.apply { mkdirs() }
                val sentinel = File(peerRoot, "keep").apply { writeText("peer data") }
                link = File(owner.paths.temp, "reset-blocker")
                owner.paths.temp.mkdirs()
                Files.createSymbolicLink(link.toPath(), sentinel.toPath())
                model.dispatch(MessengerAction.DeleteAccount)
                until("failed reset banner") { model.state.value.error != null && model.state.value.pendingReset }
                assertNull(model.session.active.value)
                assertNotNull(model.session.preferences.reset())
                assertEquals("peer data", sentinel.readText())
                assertTrue(
                    model.state.value.error!!
                        .contains("symbolic link"),
                )
                Files.delete(link.toPath())
                link = null
                compose.onNodeWithText("Retry").performClick()
                println("RESET_PROOF stage=retry-clicked pending=${model.state.value.pendingReset}")
                until("reset retry complete") { !model.state.value.pendingReset && model.state.value.error == null }
                assertNull(model.session.preferences.reset())
                assertFalse(owner.paths.root.exists())
                assertFalse(
                    model.session.preferences
                        .profiles()
                        .any { it.id == owner.profile.id },
                )
                assertTrue(
                    model.session.preferences
                        .profiles()
                        .any { it.id == peer.id },
                )
                assertEquals("peer data", sentinel.readText())

                // Exercise recovery in a new UI owner with no SDK client.
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val cold = checkNotNull(model.session.active.value)
                model.session.signOut()
                cold.paths.temp.mkdirs()
                link = File(cold.paths.temp, "cold-reset-blocker")
                Files.createSymbolicLink(link.toPath(), sentinel.toPath())
                model.session.preferences.saveReset(cold.paths.resetRecord(cold.profile.id))
                compose.runOnUiThread { compose.activity.viewModelStore.clear() }
                compose.activityRule.scenario.recreate()
                until("cold failed reset banner") { model.state.value.pendingReset && model.state.value.error != null }
                assertNull(model.session.active.value)
                assertEquals(
                    ResetPhase.DATABASE_REMOVED,
                    model.session.preferences
                        .reset()
                        ?.phase,
                )
                assertEquals("peer data", sentinel.readText())
                Files.delete(link.toPath())
                link = null
                compose.onNodeWithText("Retry").performClick()
                until(
                    "cold reset retry complete",
                ) { !model.state.value.pendingReset && model.state.value.error == null }
                assertNull(model.session.preferences.reset())
                assertFalse(cold.paths.root.exists())
                assertTrue(
                    model.session.preferences
                        .profiles()
                        .any { it.id == peer.id },
                )
                assertEquals("peer data", sentinel.readText())
                println("RESET_PROOF stage=cold-ui-retry-completed owner=false peer-preserved=true")
            } finally {
                link?.let { Files.deleteIfExists(it.toPath()) }
                cleanup()
                peerId?.let { model.session.preferences.removeProfile(it) }
                peerRoot?.deleteRecursively()
            }
        }

    @Test fun blockedAnchorEditCannotCommitAfterNavigation() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val chat = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Anchor"))
                val id = chat.sendText("Anchor body")
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                until("anchor timeline") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                val previous = model.session.preferences.anchor(owner.key.profileId, chat.id())
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Boolean>()
                model.session.preferences.beforePositionCommit = { key ->
                    if (key.contains("/scroll/")) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                model.session.preferences.positionCommitFinished = { key, accepted ->
                    if (key.contains("/scroll/")) finished.complete(accepted)
                }
                model.dispatch(MessengerAction.Viewport(ScrollAnchor(id, 42, 17, false), false))
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Navigate(Screen.APP_SETTINGS))
                release.complete(Unit)
                val accepted = withTimeout(30_000) { finished.await() }
                val actual = model.session.preferences.anchor(owner.key.profileId, chat.id())
                println("POSITION_PROOF kind=anchor accepted=$accepted previous=$previous actual=$actual")
                assertFalse(accepted)
                assertEquals(previous, actual)
                assertEquals(Screen.APP_SETTINGS, model.state.value.screen)
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    private suspend fun blockedRead(invalidate: Boolean) {
        var peer: SDKClient? = null
        var reader: Job? = null
        val release = CompletableDeferred<Unit>()
        val unregisterRelease = CompletableDeferred<Unit>()
        var stop: Deferred<Unit>? = null
        try {
            val owner = connect()
            peer =
                SDKClient.create(
                    compose.activity,
                    localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                    ClientOptions(
                        backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                        storage = StorageOptions(location = StorageLocation.InMemory),
                        deviceSync = false,
                    ),
                )
            val second = checkNotNull(peer)
            reader =
                CoroutineScope(
                    currentCoroutineContext(),
                ).launch { second.conversations.streamAllMessages().collect { } }
            val remote = second.conversations.createDm(owner.client.inboxId())
            val id = remote.sendText("Guarded incoming read")
            val stored =
                withTimeout(30_000) {
                    var value: Message? = null
                    while (value == null) {
                        value = owner.client.conversations.getMessageById(id)
                        delay(20)
                    }
                    value
                }
            val chat = checkNotNull(owner.client.conversations.getById(stored.conversationId))
            chat.updateConsentState(ConsentState.ALLOWED)
            val key = logicalConversationKey(chat, owner.client.inboxId())
            model.dispatch(MessengerAction.OpenConversation(chat.id()))
            until("read timeline") {
                model.state.value.messages
                    .any { it.id == id }
            }
            val previous = model.session.preferences.marker(owner.key.profileId, key)
            val entered = CompletableDeferred<Unit>()
            val finished = CompletableDeferred<Boolean>()
            model.session.preferences.beforePositionCommit = { value ->
                if (value.contains("/read/")) {
                    entered.complete(Unit)
                    release.await()
                }
            }
            model.session.preferences.positionCommitFinished = { value, accepted ->
                if (value.contains("/read/")) finished.complete(accepted)
            }
            model.foreground(true)
            model.dispatch(MessengerAction.Viewport(ScrollAnchor(id, stored.sentAt.ns, 0, true), true))
            withTimeout(30_000) { entered.await() }
            if (invalidate) {
                val unregisterEntered = CompletableDeferred<Unit>()
                val invalidated = CompletableDeferred<Unit>()
                model.session.onSessionInvalidated = { invalidated.complete(Unit) }
                model.session.unregisterNotifications = {
                    unregisterEntered.complete(Unit)
                    unregisterRelease.await()
                }
                stop = CoroutineScope(currentCoroutineContext()).async { model.session.signOut() }
                withTimeout(30_000) { invalidated.await() }
                assertFalse(model.session.accepts(owner.key))
                println("POSITION_PROOF stage=generation-invalidated-before-marker-release")
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                withTimeout(30_000) { unregisterEntered.await() }
            } else {
                model.foreground(false)
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
            }
            val accepted = finished.await()
            val actual = model.session.preferences.marker(owner.key.profileId, key)
            println(
                "POSITION_PROOF kind=read invalidated=$invalidate accepted=$accepted previous=$previous actual=$actual",
            )
            assertFalse(accepted)
            assertEquals(previous, actual)
        } finally {
            release.complete(Unit)
            unregisterRelease.complete(Unit)
            stop?.await()
            reader?.cancelAndJoin()
            withContext(NonCancellable) { peer?.end() }
            cleanup()
        }
    }

    @Test fun blockedReadEditCannotCommitAfterBackgrounding() = runBlocking { blockedRead(false) }

    @Test fun blockedReadEditCannotCommitAfterSessionInvalidation() = runBlocking { blockedRead(true) }
}
