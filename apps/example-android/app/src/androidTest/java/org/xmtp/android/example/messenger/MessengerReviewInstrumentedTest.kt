package org.xmtp.android.example.messenger

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

class MessengerReviewInstrumentedTest {
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
            throw AssertionError("Stage did not finish: $stage", error)
        }
    }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        until("signed-out UI") { model.state.value.screen == Screen.START }
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
        val owner = checkNotNull(model.session.active.value)
        until("connected UI") {
            model.state.value.inbox == owner.client.inboxId() && model.state.value.screen == Screen.CONVERSATIONS
        }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.session.preferences.beforePositionCommit = {}
            model.session.preferences.positionCommitFinished = { _, _ -> }
            model.conversations.writeConsent = { chat, value -> chat.updateConsentState(value) }
            model.conversations.onConsentFinished = {}
            model.chat.beforeQueuedAction = {}
            model.projection.beforeActiveRefresh = {}
            model.group.beforeGroupWrite = { _, _ -> }
            model.group.onGroupActionFinished = {}
            model.conversations.beforeConsentDecision = {}
            model.chat.onQueuedActionFinished = {}
            model.chat.actionMessageRead = { owner, id -> owner.client.conversations.getMessageById(id) }
            model.chat.historyPageRead = { chat, options, before, after -> chat.historyPage(options, before, after) }
            model.sends.messageRead = { client, id -> client.conversations.getMessageById(id) }
            graphRule.observers.onSessionInvalidated = {}
            graphRule.observers.beforeUnregister = {}
            if (model.session.active.value != null ||
                model.session.preferences.reset() != null
            ) {
                model.session.deleteAccount()
            }
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun stoppedActivityReleasesTheDefaultReader() =
        runBlocking {
            try {
                val owner = connect()
                withTimeout(30_000) { model.session.connection.first { it == "CONNECTED" } }
                compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
                withTimeout(30_000) { model.session.connection.first { it.isEmpty() } }
                assertFalse("A stopped activity keeps no default reader", defaultReaderOwned(owner.client))
                compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
                withTimeout(30_000) { model.session.connection.first { it == "CONNECTED" } }
                assertTrue("A started activity owns delivery again", defaultReaderOwned(owner.client))
                assertNull(model.session.readerError.value)
            } finally {
                cleanup()
            }
        }
}

// A second default reader fails at open while another reader owns delivery.
internal suspend fun defaultReaderOwned(client: SDKClient): Boolean =
    try {
        withTimeoutOrNull(1_000) { client.conversations.streamAllMessages().first() }
        false
    } catch (error: XmtpException.ConsumerOwned) {
        true
    }
