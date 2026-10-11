package org.xmtp.android.example.messenger.attachments

import android.app.Activity
import android.app.Instrumentation
import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.TestAppGraphRule
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.attachments.AttachmentAction
import uniffi.xmtp_sdk.*

class AttachmentSupportInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val previousLifecycle = AndroidStreamLifecycle.enabled

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    private suspend fun connect(backend: String): Conversation {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(backend, "", true)
        val owner = checkNotNull(model.session.active.value)
        until { model.state.value.inbox == owner.client.inboxId() }
        model.foreground(false)
        val group =
            Conversation.Group(
                owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Support proof")),
            )
        val opened = CompletableDeferred<Unit>()
        model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
        model.dispatch(MessengerAction.OpenConversation(group.id()))
        withTimeout(30_000) { opened.await() }
        return group
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            graphRule.graph.attachments.beforeSupportAdmission = { _, _ -> }
            model.chat.onOpenFinished = {}
            if (model.session.active.value != null ||
                model.session.preferences.reset() != null
            ) {
                model.session.deleteAccount()
            }
            model.session.signOut()
            AndroidStreamLifecycle.enabled = previousLifecycle
        }

    @Test fun unsupportedServiceHidesSelectAndCannotLaunchTheProductionPicker() =
        runBlocking<Unit> {
            val instrumentation = InstrumentationRegistry.getInstrumentation()
            val backend =
                checkNotNull(InstrumentationRegistry.getArguments().getString("unsupportedBackendUrl")) {
                    "Supply the owned unsupported backend fixture"
                }
            val monitor =
                object : Instrumentation.ActivityMonitor() {
                    override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? =
                        if (intent.action == Intent.ACTION_OPEN_DOCUMENT) {
                            Instrumentation.ActivityResult(Activity.RESULT_CANCELED, null)
                        } else {
                            null
                        }
                }
            instrumentation.addMonitor(monitor)
            val sender = AttachmentTestFixture()
            try {
                val group = connect(backend)
                val owner = checkNotNull(model.session.active.value)
                assertNull(owner.client.serverConfiguration().attachments)
                assertFalse(owner.client.attachments().offered())
                compose.onNodeWithContentDescription("Select file").assertDoesNotExist()
                graphRule.graph.dispatchAttachment(AttachmentAction.Select)
                compose.waitForIdle()
                assertEquals(0, monitor.hits)
                sender.start()
                val pending =
                    sender.client.attachments().create(
                        AttachmentSource.Bytes("retained download".toByteArray(), "retained.txt", "text/plain"),
                    )
                pending.upload()
                val id = group.sendRemoteAttachment(pending.remoteAttachment(), SendOptions(optimistic = true))
                group.publishMessage(id)
                model.dispatch(MessengerAction.Refresh)
                compose.waitUntil(30_000) { compose.onAllNodesWithText("Download").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Download").performClick()
                compose.waitUntil(30_000) { compose.onAllNodesWithText("Verified").fetchSemanticsNodes().isNotEmpty() }
                compose.onNodeWithText("Open").assertIsDisplayed()
                compose.onNodeWithText("Save").assertIsDisplayed()
                compose.onNodeWithContentDescription("Select file").assertDoesNotExist()
                println("ATTACHMENT_SUPPORT_PROOF stage=unsupported-native-config picker-hidden=true picker-launches=0")
            } finally {
                instrumentation.removeMonitor(monitor)
                sender.close()
                cleanup()
            }
        }

    @Test fun oldSupportResultCannotChangeReplacementSessionAvailability() =
        runBlocking<Unit> {
            val backend =
                checkNotNull(InstrumentationRegistry.getArguments().getString("unsupportedBackendUrl")) {
                    "Supply the owned unsupported backend fixture"
                }
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            var refreshing: Deferred<Unit>? = null
            try {
                connect(BuildConfig.XMTP_BACKEND_URL)
                val attachments = graphRule.graph.attachments
                until { attachments.availability.value.available }
                compose.onNodeWithContentDescription("Select file").assertIsDisplayed()
                val owner = checkNotNull(model.session.active.value)
                val oldToken = model.chat.screenToken()
                refreshing = async(start = CoroutineStart.LAZY) { attachments.refresh(owner, oldToken) }
                attachments.beforeSupportAdmission = { supported, token ->
                    if (currentCoroutineContext()[Job] === refreshing && supported && token == oldToken) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                checkNotNull(refreshing).start()
                withTimeout(30_000) { entered.await() }
                model.session.deleteAccount()
                model.session.connect(backend, "", true)
                val replacement = checkNotNull(model.session.active.value)
                assertNotEquals(owner.key, replacement.key)
                assertNull(replacement.client.serverConfiguration().attachments)
                assertFalse(replacement.client.attachments().offered())
                until { !attachments.availability.value.available }
                release.complete(Unit)
                withTimeout(30_000) { checkNotNull(refreshing).await() }
                assertFalse(
                    "The old supported result cannot enter the replacement capability",
                    attachments.availability.value.available,
                )
                attachments.refresh(replacement)
                until {
                    model.state.value.inbox == replacement.client.inboxId() &&
                        !model.state.value.features.attachments
                }
                val group =
                    replacement.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "New capability owner"),
                    )
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                assertFalse(attachments.availability.value.available)
                compose.onNodeWithContentDescription("Select file").assertDoesNotExist()
                println("ATTACHMENT_SUPPORT_PROOF stage=old-session-result-rejected picker-hidden=true")
            } finally {
                release.complete(Unit)
                refreshing?.cancelAndJoin()
                cleanup()
            }
        }
}
