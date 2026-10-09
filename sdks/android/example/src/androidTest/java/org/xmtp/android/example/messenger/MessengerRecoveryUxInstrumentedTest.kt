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
import java.io.IOException
import java.net.URI

class MessengerRecoveryUxInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (
            error: TimeoutCancellationException,
        ) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun chat(): Pair<ActiveSession, Group> {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
        val owner = checkNotNull(model.session.active.value)
        until("connected UI") { model.state.value.inbox == owner.client.inboxId() }
        val group =
            owner.client.conversations.createGroup(
                emptyList(),
                CreateGroupOptions(name = "Recovery composer"),
            )
        model.dispatch(MessengerAction.OpenConversation(group.id()))
        until("current chat") { model.state.value.conversationId == group.id() }
        model.foreground(false)
        return owner to group
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.session.beforeClientBuild = {}
            model.session.preferences.beforeDraftCommit = {}
            model.sends.beforePublish = {}
            model.onQueuedActionFinished = {}
            model.inspectBackend = {
                SDKClient.fetchServerConfiguration(BackendSource.Options(BackendOptions(url = it)))
            }
            if (model.session.active.value != null) model.session.deleteAccount()
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun composerKeepsDefiniteFailureAndNewEditsUntilActualAcceptance() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val (owner, group) = chat()
                model.session.preferences.beforeDraftCommit = {
                    throw IOException("Definite queue preparation failure")
                }
                compose.onNodeWithText("Message").performTextInput("kept draft")
                compose.onNodeWithText("Send").performClick()
                until("definite failure result") {
                    model.state.value.textSendResult
                        ?.accepted == false
                }
                compose.onNodeWithText("kept draft").assertExists()
                assertEquals(0uL, Conversation.Group(group).countMessages(publishedSelection()))
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty(),
                )
                val entered = CompletableDeferred<Unit>()
                var commits = 0
                model.session.preferences.beforeDraftCommit = {
                    commits += 1
                    if (commits == 1) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                compose.onNodeWithText("Send").performClick()
                withTimeout(30_000) { entered.await() }
                compose.onNode(hasSetTextAction() and hasText("kept draft")).performTextReplacement("new edit")
                release.complete(Unit)
                until("real native acceptance/publication") {
                    Conversation.Group(group).messages(publishedSelection()).any {
                        it.toRow(owner.client.inboxId()).text == "kept draft"
                    }
                }
                until("accepted result") {
                    model.state.value.textSendResult
                        ?.accepted == true
                }
                compose.onNodeWithText("new edit").assertExists()
                assertEquals(1uL, Conversation.Group(group).countMessages(publishedSelection()))
                println("COMPOSER_PROOF stage=preaccept-failure-kept-draft-native-acceptance-preserved-new-edit")
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun acceptedPublicationFailureClearsOnlyAcceptedTextAndRetriesItsNativeId() =
        runBlocking {
            try {
                val (owner, group) = chat()
                model.sends.beforePublish = { throw IOException("Publication failure after native acceptance") }
                compose.onNodeWithText("Message").performTextInput("accepted body")
                compose.onNodeWithText("Send").performClick()
                until("publication error") {
                    model.state.value.error
                        ?.contains("Publication failure") == true
                }
                val drafts = model.session.preferences.drafts(owner.key.profileId)
                val id = checkNotNull(drafts.single().acceptedMessageId)
                val accepted = checkNotNull(owner.client.conversations.getMessageById(id))
                assertEquals("accepted body", accepted.toRow(owner.client.inboxId()).text)
                compose.onNode(hasSetTextAction() and hasText("accepted body")).assertDoesNotExist()
                compose.onNodeWithText("Message").assertExists()
                model.sends.beforePublish = {}
                model.dispatch(MessengerAction.RetrySend(id))
                until("same native ID published") {
                    owner.client.conversations
                        .getMessageById(id)
                        ?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                assertEquals(listOf(id), Conversation.Group(group).messages(publishedSelection()).map { it.id })
                println("COMPOSER_PROOF stage=accepted-publication-error-cleared-body-and-retried-original-id")
            } finally {
                cleanup()
            }
        }

    @Test fun startRetryUsesCurrentBackendAndCredentialInRealConnect() =
        runBlocking {
            try {
                model.session.signOut()
                until("Start") { model.state.value.screen == Screen.START }
                val config =
                    SDKClient.fetchServerConfiguration(
                        BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                    )
                model.inspectBackend = { config.copy(auth = config.auth.copy(enabled = true)) }
                model.dispatch(MessengerAction.InspectBackend(BuildConfig.XMTP_BACKEND_URL))
                until("credential") { model.state.value.credentialsRequiredFor != null }
                compose.onNodeWithText("Credential").performTextInput("first secret")
                model.session.beforeClientBuild = { throw IOException("First connection failed") }
                compose.onNodeWithText("Connect").performClick()
                until("connection failure") {
                    model.state.value.error
                        ?.contains("First connection failed") == true
                }
                val url = "http://127.0.0.1:${URI(BuildConfig.XMTP_BACKEND_URL).port}"
                compose.onNodeWithText("Backend URL").performTextReplacement(url)
                until("new credential origin") { model.state.value.credentialsRequiredFor == url }
                compose.onNodeWithText("Credential").performTextReplacement("new secret")
                model.session.beforeClientBuild = {}
                compose.onNodeWithText("Retry").performClick()
                until("retried native owner") {
                    model.session.active.value
                        ?.profile
                        ?.backend == url
                }
                val owner = checkNotNull(model.session.active.value)
                val credential = model.session.secrets.read(owner.key.profileId, "credential")
                assertEquals("new secret", credential?.toString(Charsets.UTF_8))
                assertTrue(owner.client.inboxId().isNotBlank())
                println("START_RETRY_PROOF stage=current-url-and-credential-created-native-owner")
            } finally {
                cleanup()
            }
        }

    @Test fun failedStartupRestoreRetryUsesSavedBackendAndRetainedCredential() =
        runBlocking {
            val store = ViewModelStore()
            try {
                val (owner, _) = chat()
                val saved = owner.profile
                model.session.signOut()
                model.session.secrets.write(saved.id, "credential", "retained secret".toByteArray())
                model.session.preferences.setSignedIn(true)
                model.session.beforeClientBuild = { throw IOException("Saved restore failed") }
                val factory = ViewModelProvider.AndroidViewModelFactory.getInstance(compose.activity.application)
                val fresh = ViewModelProvider(store, factory)[MessengerViewModel::class.java]
                until("failed actual startup restore") {
                    fresh.state.value.error
                        ?.contains("Saved restore failed") == true
                }
                compose.runOnUiThread {
                    compose.activity.setContent {
                        MessengerScreens(fresh.state.collectAsState().value, fresh::dispatch)
                    }
                }
                compose.onNodeWithText("Backend URL").assertTextContains(saved.backend)
                model.session.beforeClientBuild = {}
                compose.onNodeWithText("Retry").performClick()
                until("saved native owner restored by Retry") { model.session.active.value != null }
                val restored = checkNotNull(model.session.active.value)
                assertEquals(saved.backend, restored.profile.backend)
                assertEquals(saved.inboxId, restored.client.inboxId())
                val retained = model.session.secrets.read(saved.id, "credential")
                assertEquals("retained secret", retained?.toString(Charsets.UTF_8))
                println("START_RETRY_PROOF stage=failed-restore-retried-saved-url-and-credential")
            } finally {
                cleanup()
                store.clear()
            }
        }
}
