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
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.MessengerAction
import uniffi.xmtp_sdk.*
import java.io.File

class AttachmentDownloadCancellationInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: suspend () -> Boolean) =
        withTimeout(30_000) {
            while (!check()) delay(20)
        }

    @Test fun cancelledSdkDownloadIsNotARetryableFailureCard() = runBlocking<Unit> { cancel("io") }

    @Test fun cancellationAfterVerificationKeepsTheVerifiedActions() = runBlocking<Unit> { cancel("verified") }

    private suspend fun cancel(stage: String) =
        coroutineScope {
            val enabled = AndroidStreamLifecycle.enabled
            val sender = AttachmentTestFixture()
            val gate = HeldS3Responses()
            val session = model.session
            val invalidated = session.onInvalidated
            val message = session.onMessage
            val action = CompletableDeferred<Job>()
            val verified = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                gate.release()
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() && model.state.value.features.attachments }
                model.foreground(false)
                sender.start()
                val bytes = "cancelled verified file".toByteArray()
                val pending =
                    sender.client.attachments().create(
                        AttachmentSource.Bytes(bytes, "cancel.txt", "text/plain"),
                    )
                pending.upload()
                val remote = pending.remoteAttachment()
                session.onInvalidated = {}
                session.onMessage = { _, _ -> }
                val options = CreateGroupOptions(name = "Cancel file")
                val chat = owner.client.conversations.createGroup(emptyList(), options)
                val id = chat.sendRemoteAttachment(remote, SendOptions(optimistic = true))
                chat.publishMessage(id)
                val opened = CompletableDeferred<Unit>()
                model.onOpenFinished = { if (it == chat.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                withTimeout(30_000) { opened.await() }
                until {
                    model.state.value.messages
                        .any { it.id == id }
                }
                val token = model.screenToken()
                val path = File(owner.client.attachments().localPath(remote))
                assertFalse(path.exists())
                val host = compose.activity.attachments
                host.beforeDownload = { action.complete(checkNotNull(currentCoroutineContext()[Job])) }
                if (stage == "io") {
                    gate.hold()
                } else {
                    host.beforeDownloadRefresh = {
                        verified.complete(Unit)
                        release.await()
                    }
                }
                model.dispatch(MessengerAction.Feature("open-file", id))
                val downloading = withTimeout(30_000) { action.await() }
                if (stage == "io") {
                    until { gate.heldGets() > 0 }
                    assertFalse("Actual SDK GET is held before verified publication", path.exists())
                    compose.onNodeWithText("Downloading").assertIsDisplayed()
                } else {
                    withTimeout(30_000) { verified.await() }
                    assertArrayEquals(bytes, path.readBytes())
                    compose.onNodeWithText("Verified").assertIsDisplayed()
                }
                downloading.cancel()
                gate.release()
                release.complete(Unit)
                withTimeout(30_000) { downloading.join() }
                assertTrue(model.acceptsScreen(owner.key, token))
                compose.waitUntil(5_000) {
                    compose.onAllNodesWithText("Downloading").fetchSemanticsNodes().isEmpty()
                }
                compose.onNodeWithText("Failed").assertDoesNotExist()
                if (stage == "verified") {
                    compose.onNodeWithText("Verified").assertIsDisplayed()
                    compose.onNodeWithText("Open").assertIsDisplayed()
                    compose.onNodeWithText("Save").assertIsDisplayed()
                    assertArrayEquals(bytes, path.readBytes())
                } else {
                    compose.onNodeWithText("Download").assertIsDisplayed()
                }
                val proof = "ATTACHMENT_DOWNLOAD_CANCEL_PROOF stage=$stage"
                println("$proof current-screen=true no-failed-card=true")
            } finally {
                release.complete(Unit)
                compose.activity.attachments.beforeDownload = {}
                compose.activity.attachments.beforeDownloadRefresh = {}
                model.onOpenFinished = {}
                session.onInvalidated = invalidated
                session.onMessage = message
                withContext(NonCancellable) {
                    gate.release()
                    sender.close()
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = enabled
                }
            }
        }
}
