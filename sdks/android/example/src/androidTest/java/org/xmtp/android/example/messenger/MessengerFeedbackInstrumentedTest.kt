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
            model.recoveryRead = { chat, options -> chat.messages(options) }
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
                model.recoveryRead = { chat, options -> chat.messages(options).filter { it.id != missing } }
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
