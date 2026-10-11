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
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatAction
import uniffi.xmtp_sdk.*
import java.io.IOException
import java.net.URI

class MessengerRecoveryUxInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
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
            model.chat.onQueuedActionFinished = {}
            model.connect.inspectBackend = {
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
                        ?.outcome is TextSendOutcome.Rejected
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
                        ?.outcome is TextSendOutcome.Accepted
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
            val releaseSend = CompletableDeferred<Unit>()
            val sendEntered = CompletableDeferred<Unit>()
            val metadataRefreshed = CompletableDeferred<Unit>()
            val refreshClaimed =
                java.util.concurrent.atomic
                    .AtomicBoolean()
            try {
                val (owner, group) = chat()
                model.chat.beforeQueuedAction = { action ->
                    if (action is ChatAction.SendText) {
                        sendEntered.complete(Unit)
                        releaseSend.await()
                    }
                }
                graphRule.observers.beforeEvent = { current, event ->
                    if (current.key == owner.key && sendEntered.isCompleted &&
                        event is ClientEvent.ConversationMetadataChanged &&
                        event.metadataChanged.groupId.contentEquals(group.id().hexToByteArray()) &&
                        refreshClaimed.compareAndSet(false, true)
                    ) {
                        model.chat.refresh(current)
                        metadataRefreshed.complete(Unit)
                    }
                }
                model.sends.beforePublish = { throw IOException("Publication failure after native acceptance") }
                compose.onNodeWithText("Message").performTextInput("accepted body")
                compose.onNodeWithText("Send").performClick()
                withTimeout(30_000) { sendEntered.await() }
                withContext(Dispatchers.IO) { group.updateName("Active submission recovery") }
                until("native metadata event refreshed active submission") { metadataRefreshed.isCompleted }
                graphRule.observers.beforeEvent = { _, _ -> }
                releaseSend.complete(Unit)
                until("publication error") {
                    model.chat.state.value.failure
                        ?.code == "send-publication"
                }
                val drafts = model.session.preferences.drafts(owner.key.profileId)
                val id = checkNotNull(drafts.single().acceptedMessageId)
                val accepted = checkNotNull(owner.client.conversations.getMessageById(id))
                assertEquals("accepted body", accepted.toRow(owner.client.inboxId()).text)
                val composer = model.chat.state.value.composer
                val projected =
                    model.state.value.chatState
                        ?.composer
                println(
                    "COMPOSER_PROOF stage=publication-failure-state " +
                        "length=${composer.text.length} revision=${composer.revision} " +
                        "submitted=${composer.result?.submittedRevision} " +
                        "projectedLength=${projected?.text?.length} projectedRevision=${projected?.revision}",
                )
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
                releaseSend.complete(Unit)
                model.chat.beforeQueuedAction = {}
                graphRule.observers.beforeEvent = { _, _ -> }
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
                model.connect.inspectBackend = { config.copy(auth = config.auth.copy(enabled = true)) }
                model.dispatch(MessengerAction.InspectBackend(BuildConfig.XMTP_BACKEND_URL))
                until("credential") { model.state.value.credentialsRequiredFor != null }
                compose.onNodeWithText("Credential").performTextInput("first secret")
                model.session.beforeClientBuild = { throw IOException("First connection failed") }
                compose.onNodeWithText("Connect").performClick()
                until("connection failure") {
                    model.connect.state.value.failure != null
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
                val restoredGraph = graphRule.replaceGraph()
                val restoredSession = restoredGraph.session
                restoredSession.secrets.write(saved.id, "credential", "retained secret".toByteArray())
                restoredSession.preferences.setSignedIn(true)
                val restoreAttempts =
                    java.util.concurrent.atomic
                        .AtomicInteger()
                restoredSession.beforeClientBuild = {
                    restoreAttempts.incrementAndGet()
                    throw IOException("Saved restore failed")
                }
                val fresh = withContext(Dispatchers.Main) { graphRule.newShell(store) }
                until("failed actual startup restore") { fresh.connect.state.value.failure != null }
                compose.runOnUiThread {
                    compose.activity.setContent {
                        MessengerScreens(fresh.state.collectAsState().value, fresh::dispatch)
                    }
                }
                compose.onNodeWithText("Backend URL").assertTextContains(saved.backend)
                compose.onNodeWithText("Retry").performClick()
                until("second actual restore failure") {
                    restoreAttempts.get() == 2 && !fresh.connect.state.value.busy &&
                        fresh.connect.state.value.failure != null
                }
                assertEquals(
                    "restore",
                    fresh.connect.state.value.failure
                        ?.code,
                )
                restoredSession.beforeClientBuild = {}
                compose.onNodeWithText("Retry").performClick()
                until("saved native owner restored by Retry") { restoredSession.active.value != null }
                val restored = checkNotNull(restoredSession.active.value)
                assertEquals(saved.backend, restored.profile.backend)
                assertEquals(saved.inboxId, restored.client.inboxId())
                val retained = restoredSession.secrets.read(saved.id, "credential")
                assertEquals("retained secret", retained?.toString(Charsets.UTF_8))
                println("START_RETRY_PROOF stage=failed-restore-retried-saved-url-and-credential")
            } finally {
                withContext(NonCancellable) {
                    withContext(Dispatchers.Main) { store.clear() }
                    val restoredSession = graphRule.graph.session
                    restoredSession.beforeClientBuild = {}
                    if (restoredSession.preferences.active() != null) restoredSession.deleteAccount()
                    restoredSession.signOut()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
}
