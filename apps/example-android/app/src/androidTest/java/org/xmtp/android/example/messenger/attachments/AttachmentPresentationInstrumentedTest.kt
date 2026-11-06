package org.xmtp.android.example.messenger.attachments

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.TestAppGraphRule
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.MessageSendState
import uniffi.xmtp_sdk.*
import java.io.ByteArrayOutputStream
import java.io.File

@RunWith(AndroidJUnit4::class)
class AttachmentPresentationInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    private val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    @Test fun ownFilesAndDownloadedReceivedFilesOpenWithoutAnotherDownload() =
        runBlocking {
            val peer = AttachmentTestFixture()
            try {
                peer.start()
                val graph = graphRule.graph
                graph.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(graph.session.active.value)
                val group = owner.client.conversations.createGroup(listOf(peer.client.inboxId()))
                peer.client.conversations.sync()
                val other = checkNotNull(peer.client.conversations.getById(group.id()))
                val ownDraft =
                    owner.client.attachments().create(
                        AttachmentSource.Bytes(imageBytes(), "own.png", "image/png"),
                    )
                ownDraft.upload()
                val ownRemote = ownDraft.remoteAttachment()
                val ownId = group.sendRemoteAttachment(ownRemote)
                owner.client.attachments().deleteLocal(ownRemote)
                val peerDraft =
                    peer.client.attachments().create(
                        AttachmentSource.Bytes("received bytes".toByteArray(), "received.txt", "text/plain"),
                    )
                peerDraft.upload()
                val remote = peerDraft.remoteAttachment()
                val receivedId = other.sendRemoteAttachment(remote)
                group.sync()
                val localProbe = CompletableDeferred<Unit>()
                val resumeProbe = CompletableDeferred<Unit>()
                val probeFinished = CompletableDeferred<Unit>()
                graph.attachments.beforeDisplayAdmission = { messageId ->
                    if (messageId == receivedId) {
                        localProbe.complete(Unit)
                        resumeProbe.await()
                    }
                }
                graph.attachments.afterDisplayAdmission = { messageId ->
                    if (messageId == receivedId) probeFinished.complete(Unit)
                }
                graph.destinations.navigate(AppRoute.Chat(group.id()))
                until("own attachment is openable without Download") {
                    graph.attachments.state.value.messages[ownId]
                        ?.canOpen == true
                }
                val image = compose.onNodeWithTag("attachment-card-$ownId")
                compose.onNodeWithText("own.png").assertDoesNotExist()
                image.performClick()
                compose.onNodeWithTag("attachment-metadata-$ownId", useUnmergedTree = true).assertExists()
                compose.onNodeWithText("own.png").assertDoesNotExist()
                compose.onNodeWithText("Open").assertDoesNotExist()
                image.performTouchInput { longClick() }
                compose.onNodeWithText("Details", useUnmergedTree = true).performClick()
                compose.onNodeWithText("own.png").assertExists()
                compose.onNodeWithText("640 × 480").assertExists()
                compose.onNodeWithText("Done").performClick()
                withTimeout(30_000) { localProbe.await() }
                val received = compose.onNodeWithTag("attachment-card-$receivedId")
                received.assertExists()
                assertFalse(File(owner.client.attachments().localPath(remote)).exists())
                compose.onNodeWithText("Download").assertDoesNotExist()
                received.performTouchInput { longClick() }
                compose.onNodeWithText("Download", useUnmergedTree = true).performClick()
                until("received attachment downloaded") {
                    graph.attachments.state.value.messages[receivedId]
                        ?.canOpen ==
                        true
                }
                assertEquals("received bytes", File(owner.client.attachments().localPath(remote)).readText())
                // A stale absent-file probe must not erase the newer manual download.
                resumeProbe.complete(Unit)
                withTimeout(30_000) { probeFinished.await() }
                assertTrue(
                    graph.attachments.state.value.messages[receivedId]
                        ?.canOpen == true,
                )
                assertTrue(receivedId in graph.attachments.verifiedMessageIds())
                graph.attachments.beforeDisplayAdmission = {}
                graph.attachments.afterDisplayAdmission = {}
                graph.destinations.navigate(AppRoute.Chats)
                until("chat left and view cache cleared") {
                    graph.destinations.state.value.route == AppRoute.Chats &&
                        graph.attachments.state.value.messages[receivedId] == null
                }
                compose.waitForIdle()
                compose.onNodeWithText("Message requests", substring = true).assertDoesNotExist()
                graph.destinations.navigate(AppRoute.Chat(group.id()))
                until("received file restored on revisit") {
                    graph.attachments.state.value.messages[receivedId]
                        ?.canOpen ==
                        true
                }
                val restored = compose.onNodeWithTag("attachment-card-$receivedId")
                restored.performClick()
                compose.onNodeWithText("Attachment details").assertExists()
                compose.onNodeWithText("Open").assertDoesNotExist()
                compose.onNodeWithText("Done").performClick()
                restored.performTouchInput { longClick() }
                compose.onNodeWithText("Open", useUnmergedTree = true).assertExists()
                compose.onNodeWithText("Download", useUnmergedTree = true).assertDoesNotExist()
            } finally {
                peer.close()
            }
        }

    @Test fun sharedMediaKeepsOnlyViewportPreviewsAndRestoresEveryImageWhileScrolling() =
        runBlocking {
            val graph = graphRule.graph
            graph.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            val owner = checkNotNull(graph.session.active.value)
            val group = owner.client.conversations.createGroup(emptyList())
            val draft =
                owner.client.attachments().create(
                    AttachmentSource.Bytes(imageBytes(1024, 1024), "alpha.png", "image/png"),
                )
            val remote = draft.remoteAttachment()
            repeat(50) { group.sendRemoteAttachment(remote) }
            graph.destinations.navigate(AppRoute.ConversationDetails(group.id()))
            until(
                "all shared images loaded into the settings projection",
            ) { graph.group.state.value.sharedMedia.size == 50 }
            val ids =
                graph.group.state.value.sharedMedia
                    .map { it.id }
            val list = compose.onNodeWithTag("conversation-settings")
            for (id in ids + ids.first()) {
                list.performScrollToNode(hasTestTag("shared-media-$id"))
                until("visible image pixels restored for $id") {
                    val visible = graph.attachments.visiblePreviewIds()
                    val previews = graph.attachments.previews.value
                    id in visible && visible.all { it in previews }
                }
                compose.onNodeWithTag("attachment-card-$id").assertIsDisplayed()
                val previews = graph.attachments.previews.value
                val bytes = previews.values.sumOf { it.allocationByteCount.toLong() }
                assertTrue(
                    "Preview allocation grew to $bytes bytes for ${previews.size} images",
                    bytes <= 16 * 1024 * 1024L,
                )
                val preview = checkNotNull(previews[id])
                assertEquals(1024, preview.width)
                val pixel = preview.getPixel(0, 0)
                assertEquals(128, Color.alpha(pixel))
                assertTrue(Color.blue(pixel) in 128..132)
            }
        }

    @Test fun receivedRestoreDoesNotFetchWhenItsLocalFileIsDeleted() =
        runBlocking {
            val peer = AttachmentTestFixture()
            try {
                peer.start()
                val graph = graphRule.graph
                graph.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(graph.session.active.value)
                val group = owner.client.conversations.createGroup(listOf(peer.client.inboxId()))
                peer.client.conversations.sync()
                val other = checkNotNull(peer.client.conversations.getById(group.id()))
                val draft =
                    peer.client.attachments().create(
                        AttachmentSource.Bytes("local only".toByteArray(), "local.txt", "text/plain"),
                    )
                draft.upload()
                val remote = draft.remoteAttachment()
                val id = other.sendRemoteAttachment(remote)
                group.sync()
                val local =
                    File(
                        owner.client
                            .attachments()
                            .download(remote)
                            .path,
                    )
                assertTrue(local.isFile)
                val finished = CompletableDeferred<Unit>()
                graph.attachments.beforeDownload = { owner.client.attachments().deleteLocal(remote) }
                graph.attachments.afterDisplayAdmission = { messageId -> if (messageId == id) finished.complete(Unit) }
                graph.destinations.navigate(AppRoute.Chat(group.id()))
                until("restore finished after deletion") { finished.isCompleted }
                assertFalse("Automatic restoration must not fetch the deleted received file", local.exists())
                assertFalse(id in graph.attachments.verifiedMessageIds())
                graph.attachments.beforeDownload = {}
                graph.attachments.afterDisplayAdmission = {}
                compose.onNodeWithTag("attachment-card-$id").performTouchInput { longClick() }
                compose.onNodeWithText("Download", useUnmergedTree = true).performClick()
                until("explicit download can fetch the deleted file") {
                    graph.attachments.state.value.messages[id]
                        ?.canOpen ==
                        true
                }
                assertEquals("local only", local.readText())
            } finally {
                peer.close()
            }
        }

    @Test fun verifiedImageStillShowsQueuedDeliveryUntilPublication() =
        runBlocking {
            val graph = graphRule.graph
            graph.session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            val owner = checkNotNull(graph.session.active.value)
            val group = owner.client.conversations.createGroup(emptyList())
            val draft =
                owner.client.attachments().create(
                    AttachmentSource.Bytes(imageBytes(), "queued.png", "image/png"),
                )
            draft.upload()
            val id = group.sendRemoteAttachment(draft.remoteAttachment(), SendOptions(optimistic = true))
            graph.destinations.navigate(AppRoute.Chat(group.id()))
            until("queued image has decoded pixels") {
                graph.attachments.previews.value[id] != null &&
                    graph.chat.state.value.messages
                        .any { it.id == id && it.sendState == MessageSendState.QUEUED }
            }
            compose.onNodeWithText("Queued").assertIsDisplayed()
            val image = compose.onNodeWithTag("attachment-card-$id")
            image.performClick()
            compose.onNodeWithText("Queued").assertIsDisplayed()
            image.performTouchInput { longClick() }
            compose.onNodeWithText("Details", useUnmergedTree = true).performClick()
            compose.onNodeWithText("Verified").assertIsDisplayed()
            compose.onNodeWithText("Delivery: Queued").assertIsDisplayed()
            compose.onNodeWithText("Done").performClick()
            group.publishMessage(id)
            graph.chat.refresh()
            until("published attachment delivery updated") {
                graph.chat.state.value.messages
                    .any { it.id == id && it.sendState == MessageSendState.SENT }
            }
            compose.onNodeWithText("Queued").assertDoesNotExist()
        }

    @Test fun extensionlessPdfRestoresItsViewerTypeFromLocalBytes() =
        runBlocking {
            val fixture = AttachmentTestFixture()
            try {
                fixture.start()
                val draft =
                    fixture.client.attachments().create(
                        AttachmentSource.Bytes("%PDF-1.7\n%%EOF".toByteArray(), "report", "application/pdf"),
                    )
                draft.upload()
                val remote = draft.remoteAttachment()
                assertEquals(
                    "application/pdf",
                    fixture.client
                        .attachments()
                        .download(remote)
                        .mimeType,
                )
                fixture.reopen()
                val files = AttachmentFiles(fixture.context, fixture.key, fixture.client) { fixture.current }
                val restored = checkNotNull(files.restore("pdf", remote) { true })
                assertEquals("report", restored.filename)
                assertEquals("application/pdf", restored.mimeType)
                assertEquals("application/pdf", files.openIntent("pdf").type)
            } finally {
                fixture.close()
            }
        }

    private fun imageBytes(
        width: Int = 640,
        height: Int = 480,
    ): ByteArray {
        val image = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        return try {
            image.eraseColor(Color.argb(128, 46, 104, 130))
            ByteArrayOutputStream().use { output ->
                check(image.compress(Bitmap.CompressFormat.PNG, 100, output))
                output.toByteArray()
            }
        } finally {
            image.recycle()
        }
    }

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        assertTrue(
            stage,
            withTimeoutOrNull(30_000) {
                while (!check()) {
                    compose.waitForIdle()
                    delay(30)
                }
                true
            } == true,
        )
    }
}
