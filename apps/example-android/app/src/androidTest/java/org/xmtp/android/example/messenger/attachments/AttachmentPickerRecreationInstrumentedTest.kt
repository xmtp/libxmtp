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
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.Screen
import org.xmtp.android.example.shared.attachments.AttachmentSelection
import uniffi.xmtp_sdk.*
import java.io.File
import java.io.FileNotFoundException
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference

class AttachmentPickerRecreationInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private suspend fun until(check: suspend () -> Boolean) =
        withTimeout(30_000) {
            while (!check()) delay(20)
        }

    private suspend fun awaitCurrentPickerTarget(
        owner: ActiveSession,
        chatId: String,
    ) {
        assertTrue("The picker session must still be current", model.session.accepts(owner.key))
        assertEquals(AppRoute.Chat(chatId), graphRule.graph.destinations.state.value.route)
        until {
            assertTrue("The picker session must still be current", model.session.accepts(owner.key))
            assertEquals(AppRoute.Chat(chatId), graphRule.graph.destinations.state.value.route)
            val opening = model.chat.state.value
            if (opening.conversationId == chatId && opening.failure?.code == "chat-open") {
                assertNull("Chat open must succeed before a picker action", opening.failure)
            }
            currentChatTargetReady(model, graphRule.graph, owner, chatId) && model.state.value.chatState?.let {
                it.conversationId == chatId && it.failure == null && !it.busy &&
                    it.composer.enabled && !it.unknown && it.features.attachments
            } == true
        }
        assertEquals("Chat open must bind the picker target", chatId, model.chat.currentConversation()?.id())
        assertNull("Chat open must succeed before a picker action", model.chat.state.value.failure)
        assertFalse("Chat open must finish before a picker action", model.chat.state.value.busy)
        compose.waitForIdle()
        compose.onNodeWithText("Picker recreation").assertIsDisplayed()
        compose.onNodeWithTag("message-composer").assertIsDisplayed()
        compose.onNodeWithContentDescription("Add attachment").assertIsDisplayed().assertIsEnabled()
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

    @Test fun finishedFailedOpenDoesNotAdmitAPickerAction() =
        runBlocking<Unit> {
            val enabled = AndroidStreamLifecycle.enabled
            val session = model.session
            val lookup = model.chat.lookupConversation
            val finished = model.chat.onOpenFinished
            try {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                session.signOut()
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
                val owner = checkNotNull(session.active.value)
                until { model.state.value.inbox == owner.client.inboxId() && model.state.value.features.attachments }
                model.foreground(false)
                val chat =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Picker recreation"),
                    )
                val opened = CompletableDeferred<Unit>()
                model.chat.clear()
                model.chat.lookupConversation = { current, id ->
                    if (id == chat.id()) throw java.io.IOException("Picker target lookup failure")
                    lookup(current, id)
                }
                model.chat.onOpenFinished = { if (it == chat.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                withTimeout(30_000) { opened.await() }
                assertEquals(
                    "chat-open",
                    model.chat.state.value.failure
                        ?.code,
                )
                assertNull(model.chat.currentConversation())
                until {
                    model.state.value.chatState
                        ?.failure
                        ?.code == "chat-open"
                }
                assertEquals(chat.id(), model.state.value.conversationId)
                assertEquals(Screen.TIMELINE, model.state.value.screen)
                assertFalse(
                    "A rendered failed chat must not be a current native target",
                    currentChatTargetReady(model, graphRule.graph, owner, chat.id()),
                )
                var rejected = false
                try {
                    awaitCurrentPickerTarget(owner, chat.id())
                } catch (_: AssertionError) {
                    rejected = true
                }
                assertTrue("A finished failed open must not admit a picker action", rejected)
                assertNull(graphRule.graph.attachments.choice.value)
            } finally {
                model.chat.lookupConversation = lookup
                model.chat.onOpenFinished = finished
                withContext(NonCancellable) {
                    if (session.active.value != null || session.preferences.reset() != null) session.deleteAccount()
                    session.signOut()
                    AndroidStreamLifecycle.enabled = enabled
                }
            }
        }

    private suspend fun recreate(stage: String) =
        coroutineScope {
            val enabled = AndroidStreamLifecycle.enabled
            val session = model.session
            val invalidated = graphRule.observers.beforeInvalidate
            val message = graphRule.observers.beforeHandle
            val sender = AttachmentTestFixture()
            var destination: Uri? = null
            var outstanding = false
            var monitorRegistered = false
            val selectedMimeTypes = AtomicReference<List<String>?>(null)
            val monitor =
                object : Instrumentation.ActivityMonitor() {
                    override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
                        val selecting = intent.action == Intent.ACTION_OPEN_DOCUMENT
                        val exporting = intent.action == Intent.ACTION_CREATE_DOCUMENT
                        if (selecting) {
                            selectedMimeTypes.set(intent.getStringArrayExtra(Intent.EXTRA_MIME_TYPES)?.toList())
                        }
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
                graphRule.observers.beforeInvalidate = {}
                graphRule.observers.beforeHandle = { _, _ -> }
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
                model.chat.onOpenFinished = { if (it == chat.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                withTimeout(30_000) { opened.await() }
                awaitCurrentPickerTarget(owner, chat.id())
                val token = model.chat.screenToken()
                instrumentation.addMonitor(monitor)
                monitorRegistered = true
                if (stage == "select") {
                    compose.openAttachmentChoices()
                    compose.waitUntil(30_000) {
                        compose.onAllNodesWithTag("attachment-choice-sheet").fetchSemanticsNodes().isNotEmpty()
                    }
                    compose.onNodeWithTag("attachment-choice-sheet").assertExists()
                    val sheet = checkNotNull(graphRule.graph.attachments.choice.value)
                    instrumentation.runOnMainSync { assertTrue(graphRule.graph.back()) }
                    assertNull(graphRule.graph.attachments.choice.value)
                    assertEquals(AppRoute.Chat(chat.id()), graphRule.graph.destinations.state.value.route)
                    assertNull(model.attachments.requests.launchedSelect())
                    compose.waitUntil(30_000) {
                        compose.onAllNodesWithTag("attachment-choice-sheet").fetchSemanticsNodes().isEmpty()
                    }
                    awaitCurrentPickerTarget(owner, chat.id())
                    compose.openAttachmentChoices()
                    compose.waitUntil(30_000) {
                        graphRule.graph.attachments.choice.value
                            ?.instanceId
                            ?.let { it != sheet.instanceId } == true &&
                            compose.onAllNodesWithTag("attachment-choice-file").fetchSemanticsNodes().isNotEmpty()
                    }
                    compose.onNodeWithTag("attachment-choice-file").performScrollTo().performClick()
                    pickerReady()
                    assertEquals(listOf("*/*"), selectedMimeTypes.get())
                    assertEquals(
                        AttachmentSelection.FILE,
                        checkNotNull(model.attachments.requests.launchedSelect()).selection,
                    )
                    complete(Uri.EMPTY)
                    outstanding = false
                    until { model.attachments.requests.launchedSelect() == null }
                    awaitCurrentPickerTarget(owner, chat.id())
                    compose.openAttachmentChoices()
                    compose.waitUntil(30_000) {
                        compose.onAllNodesWithTag("attachment-choice-photo-video").fetchSemanticsNodes().isNotEmpty()
                    }
                    compose.onNodeWithTag("attachment-choice-photo-video").performScrollTo().performClick()
                } else {
                    val messageId = checkNotNull(id)
                    compose.onNodeWithTag("attachment-card-$messageId", useUnmergedTree = true).assertExists()
                    compose
                        .onNodeWithText("Download")
                        .performScrollTo()
                        .assertIsDisplayed()
                        .performClick()
                    compose.waitUntil(30_000) {
                        compose.onAllNodesWithText("Verified").fetchSemanticsNodes().isNotEmpty()
                    }
                    compose
                        .onNodeWithText("Save")
                        .performScrollTo()
                        .assertIsDisplayed()
                        .performClick()
                }
                pickerReady()
                assertTrue(outstanding)
                if (stage == "select") assertEquals(listOf("image/*", "video/*"), selectedMimeTypes.get())
                val oldModel = model
                val retained = graphRule.graph.attachments.requests
                val oldRequest = if (stage == "select") retained.launchedSelect() else retained.launchedSave()
                assertNotNull(oldRequest)
                assertEquals(owner.key, checkNotNull(oldRequest).identity.sessionKey)
                assertEquals(token, oldRequest.identity.destinationToken)
                if (oldRequest is AttachmentRequest.Select) {
                    assertEquals(AttachmentSelection.PHOTO_VIDEO, oldRequest.selection)
                }
                if (oldRequest is AttachmentRequest.Save) {
                    assertArrayEquals(bytes, File(oldRequest.downloaded.path).readBytes())
                }
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
                assertTrue(oldModel.chat.acceptsScreen(owner.key, token))
                assertSame(
                    retained,
                    ViewModelProvider(newActivity)[MessengerViewModel::class.java].attachments.requests,
                )
                assertEquals(oldRequest, if (stage == "select") retained.launchedSelect() else retained.launchedSave())
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
                    assertNull(retained.launchedSelect())
                    compose.waitUntil(30_000) {
                        compose
                            .onAllNodesWithTag("attachment-card-${draft.draftId}", useUnmergedTree = true)
                            .fetchSemanticsNodes()
                            .isNotEmpty()
                    }
                    compose
                        .onNodeWithText("Send file")
                        .performScrollTo()
                        .assertIsDisplayed()
                        .performClick()
                    until {
                        val messages = chat.messages(null)
                        messages.size == 1 && messages.single().deliveryStatus == DeliveryStatus.PUBLISHED &&
                            session.preferences.drafts(owner.key.profileId).isEmpty()
                    }
                    val sent = chat.messages(null).single()
                    val content = (sent.content as? SDKMessageContent.Standard)?.value
                    val publishedRemote = checkNotNull(content as? MessageContent.RemoteAttachment).v1
                    until {
                        session.secrets.read(owner.key.profileId, ref) == null &&
                            graphRule.graph.attachments.drafts.value
                                .none { it.id == draft.draftId } &&
                            model.state.value.messages
                                .any { it.id == sent.id }
                    }
                    assertNull(session.secrets.read(owner.key.profileId, ref))
                    compose
                        .onNodeWithText("Download")
                        .performScrollTo()
                        .assertIsDisplayed()
                        .performClick()
                    compose.waitUntil(30_000) {
                        compose.onAllNodesWithText("Verified").fetchSemanticsNodes().isNotEmpty()
                    }
                    assertArrayEquals(actual, File(owner.client.attachments().localPath(publishedRemote)).readBytes())
                    compose.onNodeWithText("Open").assertIsDisplayed()
                    compose.onNodeWithText("Save").assertIsDisplayed()
                    assertEquals(listOf(sent.id), chat.messages(null).map { it.id })
                    println(
                        "ATTACHMENT_PICKER_RECREATION_PROOF stage=select-send published-id=${sent.id} " +
                            "one-message=true verified-bytes=true",
                    )
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
                    assertNull(retained.launchedSave())
                    assertEquals(listOf(id), chat.messages(null).map { it.id })
                }
                val proof = "ATTACHMENT_PICKER_RECREATION_PROOF stage=$stage"
                println("$proof request-kept=true sdk-readback=true")
            } finally {
                if (outstanding) complete(Uri.EMPTY)
                if (monitorRegistered) instrumentation.removeMonitor(monitor)
                model.chat.onOpenFinished = {}
                graphRule.observers.beforeInvalidate = invalidated
                graphRule.observers.beforeHandle = message
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
