package org.xmtp.android.example.messenger.attachments

import android.app.Instrumentation
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.provider.MediaStore
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleCallback
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
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
import java.io.FileNotFoundException
import java.util.UUID

class AttachmentPickerRecreationInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private suspend fun until(check: suspend () -> Boolean) =
        withTimeout(30_000) {
            while (!check()) delay(20)
        }

    private suspend fun observed(check: suspend () -> Boolean): Boolean {
        repeat(100) {
            if (check()) return true
            delay(50)
        }
        return check()
    }

    private fun complete(uri: Uri) {
        val request =
            Intent(AttachmentPickerActivity.COMPLETE)
                .setClassName(
                    instrumentation.context.packageName,
                    AttachmentPickerActivity.ResultReceiver::class.java.name,
                ).setData(uri)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        if (uri == Uri.EMPTY) {
            request.setData(null)
            request.flags = 0
        } else {
            request.clipData = ClipData.newRawUri("Selected file", uri)
        }
        instrumentation.targetContext.sendBroadcast(request)
    }

    private suspend fun pickerReady() {
        until {
            val root = instrumentation.uiAutomation.rootInActiveWindow
            root?.findAccessibilityNodeInfosByText("Attachment picker ready")?.isNotEmpty() == true
        }
    }

    @Test fun openDocumentResultSurvivesActualActivityRecreation() = runBlocking<Unit> { recreate("select") }

    @Test fun saveDestinationSurvivesRecreationAndCopiesSdkVerifiedBytes() = runBlocking<Unit> { recreate("save") }

    private suspend fun recreate(stage: String) =
        coroutineScope {
            val enabled = AndroidStreamLifecycle.enabled
            val session = model.session
            val invalidated = session.onInvalidated
            val message = session.onMessage
            val sender = AttachmentTestFixture()
            var destination: Uri? = null
            var outstanding = false
            val monitor =
                object : Instrumentation.ActivityMonitor() {
                    override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
                        val selecting = intent.action == Intent.ACTION_OPEN_DOCUMENT
                        val exporting = intent.action == Intent.ACTION_CREATE_DOCUMENT
                        if (selecting || exporting) {
                            val component = AttachmentPickerActivity::class.java.name
                            intent.setClassName(instrumentation.context.packageName, component)
                            outstanding = true
                        }
                        return null
                    }
                }
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() && model.state.value.features.attachments }
                model.foreground(false)
                session.onInvalidated = {}
                session.onMessage = { _, _ -> }
                val options = CreateGroupOptions(name = "Picker recreation")
                val chat = owner.client.conversations.createGroup(emptyList(), options)
                var id: String? = null
                val bytes = "save through recreated host".toByteArray()
                if (stage == "save") {
                    sender.start()
                    val pending =
                        sender.client.attachments().create(
                            AttachmentSource.Bytes(bytes, "save.txt", "text/plain"),
                        )
                    pending.upload()
                    id = chat.sendRemoteAttachment(pending.remoteAttachment(), SendOptions(optimistic = true))
                    chat.publishMessage(id)
                }
                val opened = CompletableDeferred<Unit>()
                model.onOpenFinished = { if (it == chat.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                withTimeout(30_000) { opened.await() }
                val token = model.screenToken()
                instrumentation.addMonitor(monitor)
                if (stage == "select") {
                    compose.onNodeWithContentDescription("Select file").performClick()
                } else {
                    val messageId = checkNotNull(id)
                    model.dispatch(MessengerAction.Feature("open-file", messageId))
                    compose.waitUntil(5_000) {
                        compose.onAllNodesWithText("Verified").fetchSemanticsNodes().isNotEmpty()
                    }
                    compose.onNodeWithText("Save").performClick()
                }
                pickerReady()
                assertTrue(outstanding)
                val oldModel = model
                val retained = ViewModelProvider(compose.activity)[AttachmentRequests::class.java]
                val oldRequest = if (stage == "select") retained.picker else retained.destination
                assertNotNull(oldRequest)
                val oldActivity = compose.activity
                val recreated = CompletableDeferred<MainActivity>()
                val lifecycle = ActivityLifecycleMonitorRegistry.getInstance()
                val callback =
                    ActivityLifecycleCallback { activity, state ->
                        if (activity is MainActivity && activity !== oldActivity && state == Stage.STARTED) {
                            recreated.complete(activity)
                        }
                    }
                lifecycle.addLifecycleCallback(callback)
                val newActivity =
                    try {
                        instrumentation.runOnMainSync { oldActivity.recreate() }
                        withTimeout(30_000) { recreated.await() }
                    } finally {
                        lifecycle.removeLifecycleCallback(callback)
                    }
                pickerReady()
                assertSame(oldModel, ViewModelProvider(newActivity)[MessengerViewModel::class.java])
                assertTrue(oldModel.acceptsScreen(owner.key, token))
                assertSame(retained, ViewModelProvider(newActivity)[AttachmentRequests::class.java])
                assertEquals(oldRequest, if (stage == "select") retained.picker else retained.destination)
                if (stage == "select") {
                    val authority = instrumentation.context.packageName + ".file-source"
                    complete(Uri.parse("content://$authority/file?bytes=100001&length=0"))
                    outstanding = false
                    val delivered = observed { session.preferences.drafts(owner.key.profileId).size == 1 }
                    assertTrue("Recreated OpenDocument result creates its retained conversation draft", delivered)
                    val draft = session.preferences.drafts(owner.key.profileId).single()
                    assertEquals(chat.id(), draft.conversationKey)
                    val ref = checkNotNull(draft.descriptorSecretRef)
                    val encoded = checkNotNull(session.secrets.read(owner.key.profileId, ref))
                    val remote = AttachmentDescriptor.decode(encoded)
                    val actual = File(owner.client.attachments().localPath(remote)).readBytes()
                    assertArrayEquals(ByteArray(100001) { (it % 8192 % 251).toByte() }, actual)
                    assertEquals(
                        PendingAttachmentStatus.Waiting,
                        owner.client
                            .attachments()
                            .pending(remote)
                            .status(),
                    )
                    assertNull(retained.picker)
                } else {
                    val values =
                        android.content.ContentValues().apply {
                            put(MediaStore.Downloads.DISPLAY_NAME, "recreation-${UUID.randomUUID()}.txt")
                            put(MediaStore.Downloads.MIME_TYPE, "text/plain")
                            put(MediaStore.Downloads.IS_PENDING, 0)
                        }
                    val collection = MediaStore.Downloads.EXTERNAL_CONTENT_URI
                    destination = checkNotNull(newActivity.contentResolver.insert(collection, values))
                    checkNotNull(newActivity.contentResolver.openOutputStream(checkNotNull(destination))).close()
                    newActivity.grantUriPermission(
                        instrumentation.context.packageName,
                        checkNotNull(destination),
                        Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
                    )
                    complete(checkNotNull(destination))
                    outstanding = false
                    val copied =
                        observed {
                            try {
                                compose.activity.contentResolver.openInputStream(checkNotNull(destination))?.use {
                                    it.readBytes().contentEquals(bytes)
                                } == true
                            } catch (_: FileNotFoundException) {
                                false
                            }
                        }
                    assertTrue("Recreated Save copies actual SDK-verified bytes to its selected URI", copied)
                    assertNull(retained.destination)
                    assertEquals(listOf(id), chat.messages(null).map { it.id })
                }
                val proof = "ATTACHMENT_PICKER_RECREATION_PROOF stage=$stage"
                println("$proof request-kept=true sdk-readback=true")
            } finally {
                if (outstanding) complete(Uri.EMPTY)
                instrumentation.removeMonitor(monitor)
                model.onOpenFinished = {}
                session.onInvalidated = invalidated
                session.onMessage = message
                withContext(NonCancellable) {
                    destination?.let {
                        instrumentation.targetContext.revokeUriPermission(
                            it,
                            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
                        )
                        instrumentation.targetContext.contentResolver.delete(it, null, null)
                    }
                    sender.close()
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = enabled
                }
            }
        }
}
