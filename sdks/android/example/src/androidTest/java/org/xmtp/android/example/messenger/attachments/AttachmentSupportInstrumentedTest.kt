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
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

class AttachmentSupportInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val previousLifecycle = AndroidStreamLifecycle.enabled
    private var previousInvalidated: (suspend (org.xmtp.android.example.messenger.ActiveSession) -> Unit)? = null

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
        model.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
        model.dispatch(MessengerAction.OpenConversation(group.id()))
        withTimeout(30_000) { opened.await() }
        return group
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            compose.activity.attachments.beforeSupportAdmission = { _, _ -> }
            previousInvalidated?.let { model.session.onInvalidated = it }
            model.onOpenFinished = {}
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
                model.featureAction(MessengerAction.Feature("select-file"))
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

    @Test fun oldSupportResultCannotChangeTheNewScreenAvailability() =
        runBlocking<Unit> {
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val oldResult = CompletableDeferred<Job>()
            val currentResults = CompletableDeferred<Unit>()
            var refreshing: Deferred<Unit>? = null
            try {
                connect(BuildConfig.XMTP_BACKEND_URL)
                until { model.state.value.features.attachments }
                compose.onNodeWithContentDescription("Select file").assertIsDisplayed()
                val owner = checkNotNull(model.session.active.value)
                previousInvalidated = model.session.onInvalidated
                model.session.onInvalidated = {}
                val oldToken = model.screenToken()
                compose.activity.attachments.beforeSupportAdmission = { supported, token ->
                    if (token == oldToken) {
                        assertTrue(supported)
                        oldResult.complete(checkNotNull(currentCoroutineContext()[Job]))
                        entered.complete(Unit)
                        release.await()
                    } else {
                        // Hold later valid results while this case checks the captured old result.
                        currentResults.await()
                    }
                }
                refreshing = async { model.featureRefresh(owner, model.currentConversation()) }
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Navigate(Screen.APP_SETTINGS))
                model.setFeatures(
                    model.state.value.features
                        .copy(attachments = false),
                )
                release.complete(Unit)
                withTimeout(30_000) { oldResult.await().join() }
                assertFalse(model.state.value.features.attachments)
                model.dispatch(MessengerAction.Navigate(Screen.TIMELINE))
                compose.waitUntil(5_000) {
                    compose.onAllNodesWithContentDescription("Select file").fetchSemanticsNodes().isEmpty()
                }
                assertFalse(model.state.value.features.attachments)
                compose.onNodeWithContentDescription("Select file").assertDoesNotExist()
                println("ATTACHMENT_SUPPORT_PROOF stage=old-screen-result-rejected picker-hidden=true")
            } finally {
                release.complete(Unit)
                currentResults.complete(Unit)
                refreshing?.cancelAndJoin()
                cleanup()
            }
        }
}
