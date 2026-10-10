package org.xmtp.android.example.messenger

import androidx.activity.compose.setContent
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.net.URI

class MessengerFeedbackInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun cleanup(profiles: List<BackendProfile>) =
        withContext(NonCancellable) {
            model.session.beforeSavedRestoreConnect = {}
            model.recoveryRead = { chat, options, before, after -> chat.recoveryPage(options, before, after) }
            model.onQueuedActionFinished = {}
            if (model.session.active.value != null) model.session.deleteAccount()
            for (profile in profiles.distinctBy { it.id }) {
                if (model.session.preferences
                        .profiles()
                        .any { it.id == profile.id }
                ) {
                    model.session.preferences.setActive(profile)
                    model.session.deleteAccount()
                }
            }
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun delayedStartupCannotReplaceTheUserChosenNativeBackend() =
        runBlocking {
            val store = ViewModelStore()
            val release = CompletableDeferred<Unit>()
            val owned = mutableListOf<BackendProfile>()
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(
                    BuildConfig.XMTP_BACKEND_URL,
                    "",
                    localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL),
                )
                val saved = checkNotNull(model.session.active.value).profile.also(owned::add)
                model.session.signOut()
                model.session.preferences.setSignedIn(true)
                val entered = CompletableDeferred<Unit>()
                model.session.beforeSavedRestoreConnect = {
                    assertEquals(saved.id, it.id)
                    entered.complete(Unit)
                    withContext(NonCancellable) { release.await() }
                }
                val factory =
                    ViewModelProvider.AndroidViewModelFactory.getInstance(compose.activity.application)
                val fresh = ViewModelProvider(store, factory)[MessengerViewModel::class.java]
                withTimeout(30_000) { entered.await() }
                val uri = URI(BuildConfig.XMTP_BACKEND_URL)
                val host = if (uri.host == "127.0.0.1") "10.0.2.2" else "127.0.0.1"
                val selectedUrl = "http://$host:${uri.port}"
                fresh.dispatch(MessengerAction.Connect(selectedUrl, "selected credential"))
                until("user chosen native owner") {
                    model.session.active.value
                        ?.profile
                        ?.backend == selectedUrl
                }
                val selected = checkNotNull(model.session.active.value)
                owned.add(selected.profile)
                val group =
                    selected.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Selected backend owner"),
                    )
                release.complete(Unit)
                withTimeout(30_000) { fresh.awaitStartupRestore() }
                assertEquals(
                    selected.key,
                    model.session.active.value
                        ?.key,
                )
                assertTrue(model.session.accepts(selected.key))
                assertEquals(
                    selectedUrl,
                    model.session.preferences
                        .active()
                        ?.backend,
                )
                assertEquals("Selected backend owner", group.state().name)
                val credential = model.session.secrets.read(selected.key.profileId, "credential")
                assertEquals("selected credential", credential?.toString(Charsets.UTF_8))
                until("selected backend UI") { fresh.state.value.backend == selectedUrl }
                println("STARTUP_INTENT_PROOF stage=late-saved-restore-skipped-selected-native-owner-live")
            } finally {
                release.complete(Unit)
                store.clear()
                cleanup(owned)
            }
        }

    @Test fun nativePendingPagesKeepHeldBoundsAndRetryTheRetainedId() =
        runBlocking {
            val owned = mutableListOf<BackendProfile>()
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(
                    BuildConfig.XMTP_BACKEND_URL,
                    "",
                    localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL),
                )
                val owner = checkNotNull(model.session.active.value)
                owned.add(owner.profile)
                val group = owner.client.conversations.createGroup(
                    emptyList(),
                    CreateGroupOptions(name = "Pending recovery pages"),
                )
                val chat = Conversation.Group(group)
                val accepted = (1..80).map {
                    group.sendText("Retained pending $it", SendOptions(optimistic = true))
                }
                val selection = publishedSelection().copy(deliveryStatus = null, limit = 50u)
                val newest = chat.recoveryPage(selection)
                val older = chat.recoveryPage(selection, checkNotNull(newest.lastPosition))
                assertEquals(50, newest.messages.size)
                assertEquals(30, older.messages.size)
                assertTrue(newest.hasMore)
                assertFalse(older.hasMore)
                assertEquals(accepted.toSet(), (newest.messages + older.messages).map { it.id }.toSet())
                assertEquals(0u, newest.skippedCount + older.skippedCount)

                suspend fun act(action: MessengerAction) {
                    val completed = CompletableDeferred<Unit>()
                    model.onQueuedActionFinished = { if (it == action) completed.complete(Unit) }
                    model.dispatch(action)
                    until("completed $action") { completed.isCompleted }
                    model.onQueuedActionFinished = {}
                }
                act(MessengerAction.OpenConversation(group.id()))
                assertEquals(newest.messages.map { it.id }, model.state.value.messages.map { it.id })
                assertTrue(model.state.value.hasOlderRecovery)
                act(MessengerAction.LoadOlderRecovery)
                assertEquals(older.messages.map { it.id }, model.state.value.messages.map { it.id })
                assertFalse(model.state.value.recoveryAtNewest)
                assertFalse(model.state.value.hasOlderRecovery)

                val boundaryId = newest.messages.last().id
                chat.publishMessage(boundaryId)
                val afterBoundaryPublication = chat.recoveryPage(selection, newest.lastPosition)
                assertEquals(older.messages.map { it.id }, afterBoundaryPublication.messages.map { it.id })
                val seen = java.util.concurrent.CopyOnWriteArrayList<MessageRecoveryPosition?>()
                model.recoveryRead = { current, options, before, after ->
                    seen.add(before)
                    current.recoveryPage(options, before, after)
                }
                act(MessengerAction.Refresh)
                assertEquals(listOf(newest.lastPosition), seen.toList())
                assertTrue(model.state.value.messages.any { it.id == boundaryId })
                assertEquals(
                    older.messages.map { it.id },
                    model.state.value.messages.filter { it.id != boundaryId }.map { it.id },
                )
                assertFalse(model.state.value.recoveryAtNewest)

                val retried = older.messages.last().id
                act(MessengerAction.RetrySend(retried))
                until("retained native ID published") {
                    owner.client.conversations.getMessageById(retried)?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                assertEquals(2uL, chat.countMessages(publishedSelection()))
                assertEquals(78uL, chat.countMessages(selection.copy(deliveryStatus = DeliveryStatus.UNPUBLISHED)))
                assertEquals(retried, owner.client.conversations.getMessageById(retried)?.id)
                act(MessengerAction.Refresh)
                val remaining = chat.recoveryPage(selection, newest.lastPosition)
                assertEquals(29, remaining.messages.size)
                assertEquals(
                    remaining.messages.map { it.id },
                    model.state.value.messages.filter { it.id != boundaryId && it.id != retried }.map { it.id },
                )
                println("PENDING_RECOVERY_PROOF stage=actual-eighty-pending-held-boundary-retained-id-retry")
            } finally {
                cleanup(owned)
            }
        }

    @Test fun emptyNativePendingProjectionKeepsRawContinuationAfterFourReads() =
        runBlocking {
            val owned = mutableListOf<BackendProfile>()
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(
                    BuildConfig.XMTP_BACKEND_URL,
                    "",
                    localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL),
                )
                val owner = checkNotNull(model.session.active.value)
                owned.add(owner.profile)
                val group = owner.client.conversations.createGroup(
                    emptyList(),
                    CreateGroupOptions(name = "Raw pending continuation"),
                )
                val accepted = (1..201).map {
                    group.sendText("Raw pending $it", SendOptions(optimistic = true))
                }
                val chat = Conversation.Group(group)
                val selection = publishedSelection().copy(deliveryStatus = null, limit = 50u)
                var raw = chat.recoveryPage(selection)
                repeat(4) { raw = chat.recoveryPage(selection, checkNotNull(raw.lastPosition)) }
                assertEquals(1, raw.messages.size)
                val retained = raw.messages.single().id
                assertTrue(accepted.contains(retained))
                val calls = java.util.concurrent.CopyOnWriteArrayList<MessageRecoveryPage>()
                // Simulate an unreadable prefix after real SDK reads. Keep every consumed raw position.
                model.recoveryRead = { current, options, before, after ->
                    val page = current.recoveryPage(options, before, after)
                    calls.add(page)
                    val readable = page.messages.filter { it.id == retained }
                    page.copy(
                        messages = readable,
                        skippedCount = page.skippedCount + (page.messages.size - readable.size).toUInt(),
                    )
                }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until("four raw pending pages and continuation") {
                    model.state.value.recoveryNotice != null && model.state.value.hasOlderRecovery
                }
                assertEquals("Recovery has its own four-read budget", 4, calls.size)
                assertTrue(model.state.value.messages.isEmpty())
                assertEquals(200, calls.sumOf { it.messages.size })
                compose.onNodeWithText("Older pending messages").assertIsEnabled()
                model.dispatch(MessengerAction.LoadOlderRecovery)
                until("readable native pending row after raw prefix") {
                    model.state.value.messages.map { it.id } == listOf(retained)
                }
                assertEquals(5, calls.size)
                assertFalse(model.state.value.hasOlderRecovery)
                assertEquals(1, calls.last().messages.size)
                assertEquals(retained, calls.last().messages.single().id)
                println("PENDING_RECOVERY_PROOF stage=actual-201-pages-scripted-200-projection-loss-raw-continuation")
            } finally {
                cleanup(owned)
            }
        }

    @Test fun partialPendingReadShowsANoticeAndKeepsTheNativeReadableMessage() =
        runBlocking {
            val owned = mutableListOf<BackendProfile>()
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                model.session.signOut()
                model.session.connect(
                    BuildConfig.XMTP_BACKEND_URL,
                    "",
                    localAttachmentNetwork(BuildConfig.XMTP_BACKEND_URL),
                )
                val owner = checkNotNull(model.session.active.value)
                owned.add(owner.profile)
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Partial pending rows"),
                    )
                val missing = group.sendText("Unreadable pending result", SendOptions(optimistic = true))
                val retained = group.sendText("Readable pending result", SendOptions(optimistic = true))
                val selection = publishedSelection().copy(deliveryStatus = DeliveryStatus.UNPUBLISHED)
                assertEquals(2uL, Conversation.Group(group).countMessages(selection))
                val nativeIds =
                    Conversation
                        .Group(group)
                        .messages(selection)
                        .map { it.id }
                        .toSet()
                assertEquals(setOf(missing, retained), nativeIds)
                // Simulate one conversion loss after the real SDK page read. Keep its raw bounds.
                model.recoveryRead = { chat, options, before, after ->
                    val page = chat.recoveryPage(options, before, after)
                    val readable = page.messages.filter { it.id != missing }
                    page.copy(
                        messages = readable,
                        skippedCount = page.skippedCount + (page.messages.size - readable.size).toUInt(),
                    )
                }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until("partial pending UI") {
                    val current = model.state.value
                    current.messages.any { it.id == retained } && current.recoveryNotice != null
                }
                assertFalse(model.state.value.hasOlderRecovery)
                assertEquals(
                    listOf(retained),
                    model.state.value.messages
                        .map { it.id },
                )
                val notice = "Some stored pending messages cannot be read. Refresh to try again."
                compose.onNodeWithText(notice).assertExists()
                println("PARTIAL_PENDING_PROOF stage=raw-two-readable-one-notice-with-native-row")
            } finally {
                cleanup(owned)
            }
        }
}
