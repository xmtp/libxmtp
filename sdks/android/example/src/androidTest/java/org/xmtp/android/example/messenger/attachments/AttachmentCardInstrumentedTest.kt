package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import android.graphics.Color
import android.provider.MediaStore
import androidx.activity.compose.setContent
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
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.MessengerScreens
import uniffi.xmtp_sdk.*
import java.io.ByteArrayOutputStream
import java.io.File

class AttachmentCardInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    private fun publicCaptureCount(): Int =
        compose.activity.contentResolver
            .query(
                MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                arrayOf(MediaStore.Images.Media._ID),
                "${MediaStore.Images.Media.DISPLAY_NAME} = ? AND ${MediaStore.Images.Media.RELATIVE_PATH} = ?",
                arrayOf("attachment-card-verified.png", "Pictures/XmtpMessengerProof/"),
                null,
            )?.use { it.count } ?: 0

    private fun screenshot() {
        val bitmap = checkNotNull(InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot())
        try {
            val directory = File(compose.activity.filesDir, "xmtp-messenger-proof")
            check(directory.isDirectory || directory.mkdirs())
            File(directory, "attachment-card-verified.png").outputStream().use {
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it))
            }
        } finally {
            bitmap.recycle()
        }
    }

    @Test fun verifiedDownloadUpdatesTheRealCardBeforeParentRefresh() =
        runBlocking<Unit> {
            val previousLifecycle = AndroidStreamLifecycle.enabled
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val sender = AttachmentTestFixture()
            val release = CompletableDeferred<Unit>()
            val ready = CompletableDeferred<Unit>()
            val opened = CompletableDeferred<Unit>()
            val session = model.session
            val previousInvalidated = session.onInvalidated
            val previousMessage = session.onMessage
            val previousOpenFinished = model.onOpenFinished
            val host = compose.activity.attachments
            try {
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() && model.state.value.features.attachments }
                model.foreground(false)
                sender.start()
                val image =
                    Bitmap.createBitmap(512, 256, Bitmap.Config.ARGB_8888).apply {
                        eraseColor(Color.rgb(49, 89, 232))
                    }
                val encoding = ByteArrayOutputStream()
                image.compress(Bitmap.CompressFormat.PNG, 100, encoding)
                image.recycle()
                val bytes = encoding.toByteArray()
                val pending = sender.client.attachments().create(AttachmentSource.Bytes(bytes, "blue.png", "image/png"))
                pending.upload()
                val remote = pending.remoteAttachment()
                // Hold SDK event refresh and the action's final refresh. The card must observe its own flows.
                session.onInvalidated = { release.await() }
                session.onMessage = { _, _ -> release.await() }
                val group =
                    Conversation.Group(
                        owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "File card")),
                    )
                val id = group.sendRemoteAttachment(remote, SendOptions(optimistic = true))
                group.publishMessage(id)
                model.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                until {
                    model.state.value.conversationId == group.id() &&
                        model.state.value.messages
                            .any { it.id == id }
                }
                assertFalse(File(owner.client.attachments().localPath(remote)).exists())
                host.beforeDownloadRefresh = {
                    ready.complete(Unit)
                    release.await()
                }
                val parentState = model.state.value
                compose.runOnUiThread {
                    compose.activity.setContent {
                        MessengerScreens(parentState, {}, messageExtra = { row -> host.Message(row) })
                    }
                }
                compose.onNodeWithText("Download").assertIsDisplayed()
                compose.onNodeWithText("Open").assertDoesNotExist()
                compose.onNodeWithContentDescription("blue.png").assertDoesNotExist()
                compose.onNodeWithText("Download").performClick()
                withTimeout(30_000) { ready.await() }
                assertArrayEquals(bytes, File(owner.client.attachments().localPath(remote)).readBytes())
                assertEquals(parentState, model.state.value)
                compose.waitForIdle()
                compose.onNodeWithText("Verified").assertIsDisplayed()
                compose.onNodeWithText("Open").assertIsDisplayed()
                compose.onNodeWithText("Save").assertIsDisplayed()
                compose.onNodeWithContentDescription("blue.png").assertIsDisplayed()
                println("FILE_CARD_PROOF stage=verified-actions-and-preview parent-refresh=held message=$id")
                val privateCapture =
                    File(compose.activity.filesDir, "xmtp-messenger-proof/attachment-card-verified.png")
                check(!privateCapture.exists() || privateCapture.delete())
                val publicCaptures = publicCaptureCount()
                screenshot()
                assertTrue("The card proof is stored only in the private fixture directory", privateCapture.isFile)
                assertArrayEquals(
                    byteArrayOf(0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a),
                    privateCapture.inputStream().use { input -> ByteArray(8).also { input.read(it) } },
                )
                assertEquals("The card proof does not add a public image", publicCaptures, publicCaptureCount())
            } finally {
                release.complete(Unit)
                host.beforeDownloadRefresh = {}
                session.onInvalidated = previousInvalidated
                session.onMessage = previousMessage
                model.onOpenFinished = previousOpenFinished
                withContext(NonCancellable) {
                    sender.close()
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = previousLifecycle
                }
            }
        }
}
