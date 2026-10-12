package org.xmtp.android.example.messenger.attachments

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
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
import org.xmtp.android.example.shared.MessengerAction
import uniffi.xmtp_sdk.*

class AttachmentAvailabilityReplyInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    @Test fun actualAttachmentAvailabilityRefreshKeepsConcurrentReplyAndDraftText() =
        runBlocking<Unit> {
            val previousLifecycle = AndroidStreamLifecycle.enabled
            val release = CompletableDeferred<Unit>()
            val entered = CompletableDeferred<Unit>()
            val session = model.session
            val previousInvalidated = graphRule.observers.beforeInvalidate
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() && model.state.value.features.attachments }
                model.foreground(false)
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Availability reply"),
                    )
                val id = group.sendText("Reply parent survives availability refresh")
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                until {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Message", substring = false).performTextInput("Keep the composer draft")
                graphRule.observers.beforeInvalidate = {}
                val refreshing =
                    async(Dispatchers.IO, start = CoroutineStart.LAZY) { graphRule.graph.attachments.refresh(owner) }
                graphRule.graph.attachments.beforeSupportAdmission = { supported, token ->
                    if (currentCoroutineContext()[Job] === refreshing) {
                        assertTrue(supported)
                        assertEquals(model.chat.screenToken(), token)
                        entered.complete(Unit)
                        release.await()
                    }
                }
                refreshing.start()
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Reply(id))
                until { model.state.value.replyTo == id }
                assertEquals(id, model.state.value.replyTo)
                release.complete(Unit)
                withTimeout(30_000) { refreshing.await() }
                assertEquals("Reply survives the actual attachment availability update", id, model.state.value.replyTo)
                assertEquals("Reply parent survives availability refresh", model.state.value.replyPreview)
                assertTrue(model.state.value.features.attachments)
                compose.waitUntil(5_000) { compose.onNodeWithText("Keep the composer draft").isDisplayed() }
                compose.onNodeWithText("Keep the composer draft").assertIsDisplayed()
                println("ATTACHMENT_AVAILABILITY_REPLY_PROOF stage=actual-host-refresh reply-kept=true draft-kept=true")
            } finally {
                release.complete(Unit)
                graphRule.graph.attachments.beforeSupportAdmission = { _, _ -> }
                model.chat.onOpenFinished = {}
                graphRule.observers.beforeInvalidate = previousInvalidated
                withContext(NonCancellable) {
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = previousLifecycle
                }
            }
        }
}
