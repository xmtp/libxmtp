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
import org.xmtp.android.example.messenger.app.RouteOwner
import org.xmtp.android.example.messenger.data.xmtp.BackendProbe
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.messenger.feature.connect.ConnectController
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.messenger.session.SessionCommands
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.connect.ConnectAction
import org.xmtp.android.example.shared.feature.connect.ProbeState
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

    private suspend fun chat(backend: String = BuildConfig.XMTP_BACKEND_URL): Pair<ActiveSession, Group> {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(backend, "", true)
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
                compose.onNodeWithContentDescription("Send message").performClick()
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
                compose.onNodeWithContentDescription("Send message").performClick()
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
                compose.onNodeWithContentDescription("Send message").performClick()
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
                compose
                    .onNode(
                        hasText("Retry") and hasClickAction() and hasAnyAncestor(hasTestTag("publication-recovery")),
                    ).performClick()
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
                compose
                    .onNodeWithText("Access token")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performTextInput("first secret")
                model.session.beforeClientBuild = { throw IOException("First connection failed") }
                compose
                    .onNodeWithText("Join a server")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performClick()
                until("connection failure") {
                    model.connect.state.value.failure != null
                }
                val url = "http://127.0.0.1:${URI(BuildConfig.XMTP_BACKEND_URL).port}"
                compose
                    .onNodeWithText("Server address")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performTextReplacement(url)
                until("new credential origin") { model.state.value.credentialsRequiredFor == url }
                compose
                    .onNodeWithText("Access token")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performTextReplacement("new secret")
                model.session.beforeClientBuild = {}
                compose
                    .onNodeWithText("Retry")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performClick()
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
                model.session.signOut()
                obsoleteConnectCannotClearNewRetryTarget(false)
                obsoleteConnectCannotClearNewRetryTarget(true)
            } finally {
                cleanup()
            }
        }

    private suspend fun obsoleteConnectCannotClearNewRetryTarget(retry: Boolean) {
        val job = SupervisorJob()
        val scope = CoroutineScope(job + Dispatchers.IO)
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val newerEntered = CompletableDeferred<Unit>()
        val failNewer = CompletableDeferred<Unit>()
        val retried = CompletableDeferred<Pair<String, String?>>()
        val calls =
            java.util.concurrent.atomic
                .AtomicInteger()

        class ConnectCommands : SessionCommands by model.session {
            override suspend fun restore(): Unit = throw AssertionError("Retry must retain the newer connect target")

            override suspend fun connect(
                backend: String,
                credential: String?,
                allowPrivateNetwork: Boolean,
            ) {
                val call = calls.incrementAndGet()
                when {
                    retry && call == 1 -> {
                        throw IOException("Seed retry target")
                    }

                    call == (if (retry) 2 else 1) -> {
                        entered.complete(Unit)
                        release.await()
                    }

                    call == (if (retry) 3 else 2) -> {
                        newerEntered.complete(Unit)
                        failNewer.await()
                        throw IOException("Newer connect failed")
                    }

                    else -> {
                        retried.complete(backend to credential)
                    }
                }
            }
        }
        val commands = ConnectCommands()
        val controller =
            ConnectController(
                model.session,
                commands,
                BackendProbe { error("No server probe is needed") },
                AppFeaturePreferences(compose.activity, model.session, model.session.secrets),
                RouteOwner { model.session },
                scope,
                BuildConfig.XMTP_BACKEND_URL,
                { emptyList() },
            )
        try {
            controller.dispatch(ConnectAction.Connect("https://old.example", "old secret"))
            if (retry) {
                until("seed retry failure") {
                    !controller.state.value.busy && controller.state.value.failure != null
                }
                controller.dispatch(ConnectAction.Retry)
            }
            withTimeout(30_000) { entered.await() }
            controller.dispatch(ConnectAction.Connect("https://new.example", "new secret"))
            withTimeout(30_000) { newerEntered.await() }
            release.complete(Unit)
            until("obsolete operation completed") { job.children.count() == 1 }
            failNewer.complete(Unit)
            until("newer connect failure") {
                !controller.state.value.busy && controller.state.value.failure != null
            }
            controller.dispatch(ConnectAction.Retry)
            until("newer target retried") { retried.isCompleted || !controller.state.value.busy }
            val completedAction = if (retry) "Retry" else "Connect"
            assertTrue("Retry must call connect after an obsolete $completedAction success", retried.isCompleted)
            assertEquals("https://new.example" to "new secret", retried.await())
        } finally {
            release.complete(Unit)
            failNewer.complete(Unit)
            job.cancelAndJoin()
        }
    }

    @Test fun failedStartupRestoreRetryUsesSavedBackendAndRetainedCredential() =
        runBlocking {
            val store = ViewModelStore()
            try {
                val savedBackend = "http://127.0.0.1:${URI(BuildConfig.XMTP_BACKEND_URL).port}"
                assertNotEquals(BuildConfig.XMTP_BACKEND_URL.trim().trimEnd('/'), savedBackend)
                val (owner, _) = chat(savedBackend)
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
                val config =
                    SDKClient.fetchServerConfiguration(
                        BackendSource.Options(BackendOptions(url = savedBackend)),
                    )
                restoredGraph.connect.inspectBackend = { address ->
                    config.copy(auth = config.auth.copy(enabled = address == savedBackend))
                }
                restoredGraph.connect.dispatch(ConnectAction.Inspect(BuildConfig.XMTP_BACKEND_URL))
                val fresh = withContext(Dispatchers.Main) { graphRule.newShell(store) }
                until("failed actual startup restore") {
                    fresh.connect.state.value.failure
                        ?.code == "restore"
                }
                until("saved server credential probe") {
                    val state = fresh.connect.state.value
                    state.backend == savedBackend && state.normalizedBackend == savedBackend &&
                        state.probe == ProbeState.READY && state.credentialsRequiredFor == savedBackend
                }
                fresh.connect.dispatch(ConnectAction.InspectCurrent)
                assertEquals(savedBackend, fresh.connect.state.value.backend)
                assertEquals(ProbeState.READY, fresh.connect.state.value.probe)
                compose.runOnUiThread {
                    compose.activity.setContent {
                        MessengerScreens(fresh.state.collectAsState().value, fresh::dispatch)
                    }
                }
                compose.onNodeWithText("Server address").assertTextContains(saved.backend)
                compose
                    .onNodeWithText("Access token")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                compose
                    .onNodeWithText("Retry")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performClick()
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
                compose
                    .onNodeWithText("Retry")
                    .performScrollTo()
                    .assertIsDisplayed()
                    .assertIsEnabled()
                    .performClick()
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
