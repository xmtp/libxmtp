package org.xmtp.android.example.messenger.attachments

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.shared.MessengerAction
import uniffi.xmtp_sdk.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class AttachmentAvailabilityReplyInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    @Test fun actualAttachmentAvailabilityRefreshKeepsConcurrentReplyAndDraftText() =
        runBlocking<Unit> {
            val previousLifecycle = AndroidStreamLifecycle.enabled
            val release = CountDownLatch(1)
            val entered = CompletableDeferred<Unit>()
            val session = model.session
            val previousInvalidated = session.onInvalidated
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
                model.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                until {
                    model.state.value.messages
                        .any { it.id == id }
                }
                compose.onNodeWithText("Message", substring = false).performTextInput("Keep the composer draft")
                session.onInvalidated = {}
                model.beforeFeaturesUiUpdate = {
                    entered.complete(Unit)
                    check(release.await(30, TimeUnit.SECONDS))
                }
                val refreshing = async(Dispatchers.IO) { model.featureRefresh(owner, model.currentConversation()) }
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Reply(id))
                assertEquals(id, model.state.value.replyTo)
                release.countDown()
                withTimeout(30_000) { refreshing.await() }
                assertEquals("Reply survives the actual attachment availability update", id, model.state.value.replyTo)
                assertEquals("Reply parent survives availability refresh", model.state.value.replyPreview)
                assertTrue(model.state.value.features.attachments)
                compose.onNodeWithText("Keep the composer draft").assertIsDisplayed()
                println("ATTACHMENT_AVAILABILITY_REPLY_PROOF stage=actual-host-refresh reply-kept=true draft-kept=true")
            } finally {
                release.countDown()
                model.beforeFeaturesUiUpdate = {}
                model.onOpenFinished = {}
                session.onInvalidated = previousInvalidated
                withContext(NonCancellable) {
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = previousLifecycle
                }
            }
        }
}
