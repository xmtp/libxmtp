package org.xmtp.android.example.messenger

import android.os.Build
import android.provider.Settings
import androidx.activity.BackEventCompat
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.emoji2.emojipicker.EmojiPickerView
import androidx.emoji2.text.EmojiCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import androidx.test.espresso.Espresso
import androidx.test.espresso.matcher.ViewMatchers.*
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.chat.MessageSurface
import org.xmtp.android.example.shared.feature.settings.SettingsAction
import uniffi.xmtp_sdk.*
import java.io.IOException
import java.security.SecureRandom

class MessengerScreensInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        assertTrue(
            stage,
            withTimeoutOrNull(30_000) {
                while (!check()) delay(30)
                true
            } == true,
        )
    }

    private fun composer() =
        compose.onNode(
            hasSetTextAction() and hasAnyAncestor(hasTestTag("message-composer")),
        )

    private fun textAction(label: String) = compose.onNode(hasText(label) and hasClickAction())

    private fun screen(
        name: String,
        closeKeyboard: Boolean = true,
    ) {
        if (closeKeyboard) Espresso.closeSoftKeyboard()
        compose.waitForIdle()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.waitForIdleSync()
        instrumentation.uiAutomation.waitForIdle(700, 10_000)
        val bitmap = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        try {
            writeProofScreenshot(compose.activity, name, bitmap)
        } finally {
            bitmap.recycle()
        }
    }

    private fun back() {
        Espresso.pressBack()
        compose.waitForIdle()
    }

    private suspend fun queuedConsentAdmission(
        owner: ActiveSession,
        request: Conversation,
        requestMessage: String,
    ) {
        val graph = graphRule.graph
        val session = graph.session
        val requestId = request.id()
        for (heldReaction in listOf(false, true)) {
            val revokedConsent = if (heldReaction) ConsentState.DENIED else ConsentState.UNKNOWN
            val previousDraftIds =
                session.preferences
                    .drafts(owner.key.profileId)
                    .map { it.draftId }
                    .toSet()
            val blockedText = "Draft held before native consent admission"
            val queueEntered = CompletableDeferred<Unit>()
            val releaseQueue = CompletableDeferred<Unit>()
            val actionFinished = CompletableDeferred<Unit>()
            graph.sends.afterQueueDraftCommit = {
                queueEntered.complete(Unit)
                releaseQueue.await()
            }
            graph.chat.onQueuedActionFinished = { action ->
                if ((heldReaction && action is ChatAction.React && action.emoji == "😢") ||
                    (!heldReaction && action is ChatAction.SendText && action.text == blockedText)
                ) {
                    actionFinished.complete(Unit)
                }
            }
            try {
                assertFalse(graph.chat.state.value.unknown)
                if (heldReaction) {
                    Espresso.closeSoftKeyboard()
                    compose.waitForIdle()
                    val requestRow = hasText("A real message request") and hasClickAction()
                    compose.onNodeWithTag("message-timeline").performScrollToNode(requestRow)
                    compose.onNode(requestRow).assertIsDisplayed().performClick()
                    until("held reaction opens the request message popover") {
                        graph.chat.state.value.overlay?.let {
                            it.messageId == requestMessage && it.surface == MessageSurface.POPOVER
                        } == true
                    }
                    compose.onNodeWithTag("reaction-popover").assertIsDisplayed()
                    compose.onNodeWithText("😢").assertIsEnabled().performClick()
                } else {
                    composer().performTextReplacement(blockedText)
                    compose.onNodeWithContentDescription("Send message").assertIsEnabled().performClick()
                }
                withTimeout(30_000) { queueEntered.await() }
                request.updateConsentState(revokedConsent)
                assertEquals(revokedConsent, conversationState(request).consentState)
                releaseQueue.complete(Unit)
                withTimeout(30_000) { actionFinished.await() }
                assertEquals(
                    "Queued outbound work must preserve revoked native consent",
                    revokedConsent,
                    conversationState(request).consentState,
                )
                assertFalse(
                    "Queued text must not reach the native conversation after consent changes",
                    request.messages(publishedSelection().copy(deliveryStatus = null)).any {
                        it.standardContent() == MessageContent.Text(blockedText)
                    },
                )
                assertFalse(
                    "Queued reaction must not reach the native message after consent changes",
                    checkNotNull(owner.client.conversations.getMessageById(requestMessage)).reactions.any {
                        it.reaction.content == "😢" && it.reaction.action == ReactionAction.ADDED
                    },
                )
                if (!heldReaction) {
                    assertEquals(blockedText, graph.chat.state.value.composer.text)
                    assertTrue(
                        graph.chat.state.value.composer.result
                            ?.outcome is TextSendOutcome.Rejected,
                    )
                }
                assertFalse(
                    "Rejected outbound work must not keep an uncertain or accepted send record",
                    session.preferences.drafts(owner.key.profileId).any {
                        it.conversationKey == requestId && it.draftId !in previousDraftIds &&
                            (it.phase == SendPhase.QUEUEING || it.phase == SendPhase.ACCEPTED)
                    },
                )
                if (revokedConsent == ConsentState.DENIED) request.updateConsentState(ConsentState.UNKNOWN)
                compose.runOnIdle { graph.chat.dispatch(ChatAction.Refresh) }
                until("revoked native consent appears as a request") { graph.chat.state.value.unknown }
                textAction("Accept").performClick()
                until("explicit Accept restores native consent after queued rejection") {
                    conversationState(request).consentState == ConsentState.ALLOWED &&
                        !graph.chat.state.value.unknown
                }
                if (!heldReaction) composer().performTextClearance()
            } finally {
                releaseQueue.complete(Unit)
                graph.sends.afterQueueDraftCommit = {}
                graph.chat.onQueuedActionFinished = {}
            }
        }
    }

    private suspend fun retryDeleteConsentAdmission(
        owner: ActiveSession,
        request: Conversation,
    ) {
        val graph = graphRule.graph
        for (heldDelete in listOf(false, true)) {
            val nativeId =
                request.sendText(
                    if (heldDelete) "Held deletion fixture" else "Held publication fixture",
                    SendOptions(optimistic = !heldDelete),
                )
            val nativeEntered = CompletableDeferred<Unit>()
            val releaseNative = CompletableDeferred<Unit>()
            val actionFinished = CompletableDeferred<Unit>()
            val originalRead = graph.chat.actionMessageRead
            if (heldDelete) {
                graph.chat.actionMessageRead = { active, id ->
                    val message = originalRead(active, id)
                    if (id == nativeId) {
                        nativeEntered.complete(Unit)
                        releaseNative.await()
                    }
                    message
                }
            } else {
                graph.sends.beforePublish = { id ->
                    if (id == nativeId) {
                        nativeEntered.complete(Unit)
                        releaseNative.await()
                    }
                }
            }
            graph.chat.onQueuedActionFinished = { action ->
                if ((heldDelete && action is ChatAction.DeleteMessage && action.messageId == nativeId) ||
                    (!heldDelete && action is ChatAction.RetrySend && action.messageId == nativeId)
                ) {
                    actionFinished.complete(Unit)
                }
            }
            try {
                compose.runOnIdle {
                    graph.chat.dispatch(
                        if (heldDelete) ChatAction.DeleteMessage(nativeId) else ChatAction.RetrySend(nativeId),
                    )
                }
                withTimeout(30_000) { nativeEntered.await() }
                val revokedConsent = if (heldDelete) ConsentState.UNKNOWN else ConsentState.DENIED
                request.updateConsentState(revokedConsent)
                releaseNative.complete(Unit)
                withTimeout(30_000) { actionFinished.await() }
                assertEquals(
                    "Held retry or deletion must preserve revoked native consent",
                    revokedConsent,
                    conversationState(request).consentState,
                )
                val nativeMessage = checkNotNull(owner.client.conversations.getMessageById(nativeId))
                if (heldDelete) {
                    assertFalse(
                        "Held deletion must not delete after consent changes",
                        nativeMessage.toRow(owner.client.inboxId()).deleted,
                    )
                } else {
                    assertNotEquals(
                        "Held retry must not publish after consent changes",
                        DeliveryStatus.PUBLISHED,
                        nativeMessage.deliveryStatus,
                    )
                }
                if (revokedConsent == ConsentState.DENIED) request.updateConsentState(ConsentState.UNKNOWN)
                compose.runOnIdle { graph.chat.dispatch(ChatAction.Refresh) }
                until("held native write becomes a request") { graph.chat.state.value.unknown }
                textAction("Accept").performClick()
                until("explicit Accept restores consent after held native write") {
                    conversationState(request).consentState == ConsentState.ALLOWED &&
                        !graph.chat.state.value.unknown
                }
            } finally {
                releaseNative.complete(Unit)
                graph.chat.actionMessageRead = originalRead
                graph.sends.beforePublish = {}
                graph.chat.onQueuedActionFinished = {}
            }
        }
    }

    @Test fun realSessionRendersSetupListMessagesAndSettings() =
        runBlocking {
            val graph = graphRule.graph
            val session = graph.session
            assertEquals("org.xmtp.android.example", compose.activity.packageName)
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var peer: SDKClient? = null
            var peerReader: Job? = null
            val releaseSend = CompletableDeferred<Unit>()
            val historyEntered = CompletableDeferred<Unit>()
            val releaseHistory = CompletableDeferred<Unit>()
            try {
                compose.onNodeWithContentDescription("XMTP").assertExists()
                screen("setup")
                textAction("Join a server").performScrollTo().performClick()
                until("real server connected") {
                    session.active.value != null &&
                        graph.settings.state.value.inbox
                            .isNotBlank()
                }
                val owner = checkNotNull(session.active.value)
                peer =
                    SDKClient.create(
                        compose.activity,
                        localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val second = checkNotNull(peer)
                peerReader = launch(Dispatchers.IO) { second.conversations.streamAllMessages().collect {} }
                screen("conversations")
                compose.onNodeWithContentDescription("New message").performClick()
                textAction("Group").performClick()
                compose.onNodeWithText("Inbox ID or Ethereum address").performTextInput(second.inboxId())
                textAction("Check and add").performScrollTo().performClick()
                until("native recipient resolved") { graph.createConversation.state.value.selectedRecipients.size == 1 }
                screen("create")
                textAction("Continue").performScrollTo().performClick()
                compose.onNodeWithText("Group name").performTextInput("Navy proof group")
                screen("create-group")
                graph.chat.historyPageRead = { chat, options, before, after ->
                    historyEntered.complete(Unit)
                    releaseHistory.await()
                    chat.historyPage(options, before, after)
                }
                textAction("Create group").performScrollTo().performClick()
                until("created group timeline") {
                    graph.chat.state.value.conversationId != null && historyEntered.isCompleted
                }
                compose.waitForIdle()
                compose.onNodeWithTag("message-timeline").assertIsDisplayed()
                assertTrue(
                    "The UI starts with an empty history",
                    graph.chat.state.value.messages
                        .isEmpty(),
                )
                val chatId = checkNotNull(graph.chat.state.value.conversationId)
                val group = (checkNotNull(owner.client.conversations.getById(chatId)) as Conversation.Group).group
                assertEquals("Navy proof group", group.state().name)
                until("peer joined UI-created group") { second.conversations.getById(chatId) != null }
                val peerChat = checkNotNull(second.conversations.getById(chatId))
                val parentBody = "Quote first line\nHidden second line"
                val parentId = peerChat.sendText(parentBody)
                until("actual incoming text is stored before UI history") {
                    owner.client.conversations.getMessageById(parentId) != null
                }
                graph.conversations.refresh(owner)
                until("incoming badge before UI history") { graph.conversations.row(chatId)?.unread == "1" }
                releaseHistory.complete(Unit)
                graph.chat.historyPageRead =
                    { chat, options, before, after -> chat.historyPage(options, before, after) }
                until("actual incoming text rendered") {
                    graph.chat.state.value.messages
                        .any { it.id == parentId }
                }

                compose.onNodeWithText(parentBody).assertIsDisplayed()
                until("UI history arrival marks the incoming message read without a scroll") {
                    graph.chat.state.value.atNewest && graph.conversations.row(chatId)?.unread == "0"
                }
                println("VIEWPORT_PROOF stage=empty-ui-history-arrival-marks-native-message-read")

                fun parentMessage(): SemanticsNodeInteraction {
                    val parent = hasText(parentBody) and hasClickAction() and !hasText("A reply from this device")
                    Espresso.closeSoftKeyboard()
                    compose.waitForIdle()
                    compose.onNodeWithTag("message-timeline").performScrollToNode(parent)
                    return compose.onNode(parent).assertIsDisplayed()
                }

                fun selectParent() {
                    parentMessage().performClick()
                }

                fun previewBack(edge: Int = BackEventCompat.EDGE_LEFT) {
                    compose.runOnIdle {
                        compose.activity.onBackPressedDispatcher.dispatchOnBackStarted(
                            BackEventCompat(0f, 100f, 0f, edge),
                        )
                    }
                    compose.runOnIdle {
                        compose.activity.onBackPressedDispatcher.dispatchOnBackProgressed(
                            BackEventCompat(120f, 100f, 0.6f, edge),
                        )
                    }
                    compose.waitForIdle()
                }

                fun cancelBack() {
                    compose.runOnIdle { compose.activity.onBackPressedDispatcher.dispatchOnBackCancelled() }
                    compose.waitForIdle()
                }

                fun commitBack() {
                    compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
                    compose.waitForIdle()
                }

                composer().performTextInput("Hello from this device")
                val sendEntered = CompletableDeferred<Unit>()
                graph.chat.beforeQueuedAction = { action ->
                    if (action is ChatAction.SendText && action.text == "Hello from this device") {
                        sendEntered.complete(Unit)
                        releaseSend.await()
                    }
                }
                val jobsBeforeSend = checkNotNull(owner.work.coroutineContext[Job]).children.toSet()
                compose.runOnIdle {
                    val repeated =
                        ChatAction.SendText(
                            "Hello from this device",
                            917L,
                            graph.chat.state.value.composer.revision,
                        )
                    graph.chat.dispatch(repeated)
                    graph.chat.dispatch(repeated)
                }
                withTimeout(30_000) { sendEntered.await() }
                val submittedJobs =
                    checkNotNull(owner.work.coroutineContext[Job])
                        .children
                        .filter { it !in jobsBeforeSend }
                        .toList()
                releaseSend.complete(Unit)
                withTimeout(30_000) { submittedJobs.forEach { it.join() } }
                graph.chat.beforeQueuedAction = {}
                val nativeSent =
                    Conversation
                        .Group(group)
                        .messages(publishedSelection().copy(deliveryStatus = null))
                        .filter { it.standardContent() == MessageContent.Text("Hello from this device") }
                assertEquals("Repeated send keeps one native row", 1, nativeSent.size)
                val sentId = nativeSent.single().id
                until("peer received one native text ID") {
                    peerChat.messages(publishedSelection()).any { it.id == sentId }
                }
                assertEquals(
                    listOf(sentId),
                    peerChat
                        .messages(publishedSelection())
                        .filter { it.standardContent() == MessageContent.Text("Hello from this device") }
                        .map { it.id },
                )

                parentMessage().performTouchInput { longClick() }
                until("physical long press selects the native parent") {
                    graph.chat.state.value.overlay
                        ?.messageId == parentId &&
                        graph.chat.state.value.overlay
                            ?.surface == MessageSurface.POPOVER
                }
                val clipboard =
                    checkNotNull(compose.activity.getSystemService(android.content.ClipboardManager::class.java))
                compose.runOnIdle {
                    clipboard.setPrimaryClip(android.content.ClipData.newPlainText("XMTP fixture", "before-copy"))
                }
                textAction("Copy").performClick()
                var copied: String? = null
                compose.runOnIdle {
                    copied =
                        clipboard.primaryClip
                            ?.getItemAt(0)
                            ?.coerceToText(compose.activity)
                            ?.toString()
                }
                assertEquals(parentBody, copied)
                assertNull(graph.chat.state.value.overlay)
                assertEquals(
                    parentBody,
                    checkNotNull(owner.client.conversations.getMessageById(parentId))
                        .toRow(
                            owner.client.inboxId(),
                        ).text,
                )
                composer().performTextInput("unsent draft")
                parentMessage().performTouchInput {
                    swipe(Offset(width * .25f, height * .5f), Offset(width * .75f, height * .5f), 400)
                }
                until("reply first line and draft") {
                    graph.chat.state.value.composer.reply
                        ?.messageId == parentId
                }
                assertEquals(
                    "Quote first line",
                    graph.chat.state.value.composer.reply
                        ?.firstLine,
                )
                compose.onNodeWithText("Quote first line", substring = false).assertExists()
                composer().assertTextContains("unsent draft")
                screen("ux-reply-composer")
                if (Build.VERSION.SDK_INT >= 29) {
                    val beforeBack = graph.destinations.state.value
                    val beforeComposer = graph.chat.state.value.composer
                    val beforeAnchor = graph.chat.state.value.anchor
                    val pane = compose.onNodeWithTag("back-preview-pane")
                    val beforeBounds = pane.fetchSemanticsNode().boundsInRoot
                    val resolver = compose.activity.contentResolver
                    val motionSetting = Settings.Global.ANIMATOR_DURATION_SCALE
                    val previousMotion = Settings.Global.getString(resolver, motionSetting)
                    val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
                    automation.adoptShellPermissionIdentity(android.Manifest.permission.WRITE_SECURE_SETTINGS)
                    try {
                        assertTrue(Settings.Global.putFloat(resolver, motionSetting, 1f))
                        previewBack()
                        compose.waitUntil(5_000) { pane.fetchSemanticsNode().boundsInRoot.width < beforeBounds.width }
                        assertEquals(beforeBack, graph.destinations.state.value)
                        assertEquals(beforeComposer, graph.chat.state.value.composer)
                        assertEquals(beforeAnchor, graph.chat.state.value.anchor)
                        cancelBack()
                        assertEquals(beforeBounds, pane.fetchSemanticsNode().boundsInRoot)
                        assertEquals(beforeBack, graph.destinations.state.value)
                        assertEquals(beforeComposer, graph.chat.state.value.composer)
                        assertEquals(beforeAnchor, graph.chat.state.value.anchor)

                        assertTrue(Settings.Global.putFloat(resolver, motionSetting, 0f))
                        previewBack(BackEventCompat.EDGE_RIGHT)
                        compose.waitUntil(5_000) { pane.fetchSemanticsNode().boundsInRoot == beforeBounds }
                        assertEquals(beforeBack, graph.destinations.state.value)
                        assertEquals(beforeComposer, graph.chat.state.value.composer)
                        cancelBack()
                        assertEquals(beforeBounds, pane.fetchSemanticsNode().boundsInRoot)
                        assertEquals(beforeBack, graph.destinations.state.value)
                        assertEquals(beforeComposer, graph.chat.state.value.composer)
                        assertEquals(beforeAnchor, graph.chat.state.value.anchor)
                    } finally {
                        try {
                            Settings.Global.putString(resolver, motionSetting, previousMotion)
                        } finally {
                            automation.dropShellPermissionIdentity()
                        }
                        cancelBack()
                    }
                }
                compose.onNodeWithContentDescription("Cancel reply").performClick()
                assertNull(graph.chat.state.value.composer.reply)
                compose.onNodeWithText("Quote first line", substring = false).assertDoesNotExist()
                composer().assertTextContains("unsent draft")
                compose.onNodeWithContentDescription("Send message").performClick()
                until("cancelled reply sends the retained ordinary draft") {
                    Conversation.Group(group).messages(publishedSelection()).any {
                        it.standardContent() == MessageContent.Text("unsent draft") && it.inReplyTo == null
                    }
                }
                selectParent()
                textAction("Reply").performClick()
                composer().performTextReplacement("A reply from this device")
                compose.onNodeWithContentDescription("Send message").performClick()
                until("native reply keeps incoming parent ID") {
                    Conversation.Group(group).messages(publishedSelection()).any {
                        it.standardContent() ==
                            MessageContent.Reply(parentId, MessageBody.Text("A reply from this device"))
                    }
                }

                selectParent()
                for (emoji in listOf("👍", "❤️", "😂", "😮", "😢")) {
                    val bounds = compose.onNodeWithText(emoji).getUnclippedBoundsInRoot()
                    assertTrue((bounds.right - bounds.left).value >= 48)
                    assertTrue((bounds.bottom - bounds.top).value >= 48)
                }
                screen("ux-inline-reactions")
                compose.onNodeWithText("👍").performClick()
                until("real quick reaction") {
                    graph.chat.state.value.messages.any {
                        it.id == parentId && it.reactions.any { r -> r.emoji == "👍" && r.count == 1 }
                    }
                }
                selectParent()
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithTag("emoji-sheet").assertExists()
                until("bundled native font") { EmojiCompat.get().loadState == EmojiCompat.LOAD_STATE_SUCCEEDED }
                var visibleNativeEmoji = 0
                until("native picker draws visible emoji cells") {
                    visibleNativeEmoji = 0
                    Espresso.onView(isAssignableFrom(EmojiPickerView::class.java)).check { view, error ->
                        if (error != null) throw error

                        fun visit(cell: android.view.View) {
                            val visible = android.graphics.Rect()
                            if (cell.javaClass.name == "androidx.emoji2.emojipicker.EmojiView" &&
                                !cell.contentDescription.isNullOrEmpty() && cell.isShown &&
                                cell.width > 0 && cell.height > 0 && cell.getGlobalVisibleRect(visible) &&
                                visible.width() >= cell.width / 2 && visible.height() >= cell.height / 2
                            ) {
                                val pixels =
                                    android.graphics.Bitmap.createBitmap(
                                        cell.width,
                                        cell.height,
                                        android.graphics.Bitmap.Config.ARGB_8888,
                                    )
                                try {
                                    cell.draw(android.graphics.Canvas(pixels))
                                    var painted = 0
                                    for (y in 0 until pixels.height) {
                                        for (x in 0 until pixels.width) {
                                            if (android.graphics.Color.alpha(pixels.getPixel(x, y)) > 0) painted++
                                        }
                                    }
                                    if (painted >= 32) visibleNativeEmoji++
                                } finally {
                                    pixels.recycle()
                                }
                            }
                            if (cell is android.view.ViewGroup) repeat(cell.childCount) { visit(cell.getChildAt(it)) }
                        }
                        visit(view)
                    }
                    visibleNativeEmoji >= 3
                }
                assertTrue("The native picker must draw emoji, not only category headers", visibleNativeEmoji >= 3)
                println("NATIVE_EMOJI_PROOF visibleDrawnCells=$visibleNativeEmoji")

                fun nativeGrid(): RecyclerView {
                    var grid: RecyclerView? = null
                    Espresso.onView(isAssignableFrom(EmojiPickerView::class.java)).check { view, error ->
                        if (error != null) throw error

                        fun visit(child: android.view.View) {
                            if (child is RecyclerView &&
                                child.layoutManager is androidx.recyclerview.widget.GridLayoutManager
                            ) {
                                grid = child
                            }
                            if (child is android.view.ViewGroup) {
                                repeat(
                                    child.childCount,
                                ) { visit(child.getChildAt(it)) }
                            }
                        }
                        visit(view)
                    }
                    return checkNotNull(grid)
                }
                val loadedGrid = nativeGrid()
                compose.onNodeWithTag("emoji-search").performTextInput(" ")
                until("blank search updates native picker state") {
                    graph.chat.state.value.overlay
                        ?.emojiQuery == " "
                }
                compose.waitForIdle()
                assertSame(
                    "A state update with the same columns must keep the native emoji grid",
                    loadedGrid,
                    nativeGrid(),
                )
                compose.onNodeWithTag("emoji-search").performTextClearance()
                compose.waitForIdle()
                assertSame("Clearing a blank search must keep the native emoji grid", loadedGrid, nativeGrid())
                Espresso.closeSoftKeyboard()
                Espresso.onView(isAssignableFrom(EmojiPickerView::class.java)).check { view, error ->
                    if (error != null) throw error
                    val picker = view as EmojiPickerView
                    val cell = (picker.width - picker.paddingLeft - picker.paddingRight) / picker.emojiGridColumns
                    assertTrue(cell / picker.resources.displayMetrics.density >= 48)

                    fun visit(view: android.view.View) {
                        if (view is RecyclerView &&
                            (view.layoutManager as? LinearLayoutManager)?.orientation == RecyclerView.HORIZONTAL
                        ) {
                            repeat(view.childCount) { index ->
                                val child = view.getChildAt(index)
                                assertTrue(child.width / picker.resources.displayMetrics.density >= 48)
                                assertTrue(child.height / picker.resources.displayMetrics.density >= 48)
                            }
                        }
                        if (view is android.view.ViewGroup) repeat(view.childCount) { visit(view.getChildAt(it)) }
                    }
                    visit(picker)
                }
                screen("ux-full-reactions")
                back()
                until("emoji Back retains selected popover") {
                    graph.chat.state.value.overlay
                        ?.surface == MessageSurface.POPOVER
                }
                assertEquals(
                    parentId,
                    graph.chat.state.value.overlay
                        ?.messageId,
                )
                compose.onNodeWithTag("emoji-sheet").assertDoesNotExist()
                compose.onNodeWithTag("reaction-popover").assertExists()
                val selectedBeforeBack = graph.chat.state.value.overlay
                val selectedRoute = graph.destinations.state.value
                previewBack()
                assertEquals(selectedBeforeBack, graph.chat.state.value.overlay)
                assertEquals(selectedRoute, graph.destinations.state.value)
                cancelBack()
                assertEquals(selectedBeforeBack, graph.chat.state.value.overlay)
                assertEquals(selectedRoute, graph.destinations.state.value)
                compose.onNodeWithTag("reaction-popover").assertExists()
                previewBack()
                commitBack()
                assertNull(graph.chat.state.value.overlay)
                assertEquals(AppRoute.Chat(chatId), graph.destinations.state.value.route)
                selectParent()
                compose.onNodeWithContentDescription("More reactions").performClick()
                compose.onNodeWithTag("emoji-search").performTextInput("grinning face")
                val searched = hasText("😀") and hasClickAction()
                compose.waitUntil(30_000) { compose.onAllNodes(searched).fetchSemanticsNodes().isNotEmpty() }
                compose.onAllNodes(searched).onFirst().performClick()
                until("searched emoji writes native reaction") {
                    checkNotNull(owner.client.conversations.getMessageById(parentId)).reactions.any {
                        it.reaction.content == "😀" && it.reaction.action == ReactionAction.ADDED
                    }
                }
                until("searched emoji reaches the selected native message row") {
                    graph.chat.state.value.messages.single { it.id == parentId }.reactions.any {
                        it.emoji == "😀" && it.count == 1 &&
                            it.reactors.any { reactor -> reactor.inboxId == owner.client.inboxId() }
                    }
                }
                until("searched emoji selection closes its overlay") { graph.chat.state.value.overlay == null }
                assertEquals(AppRoute.Chat(chatId), graph.destinations.state.value.route)
                compose.onNodeWithTag("emoji-sheet").assertDoesNotExist()

                fun openThumbReactors() {
                    val chip = hasText("👍 1") and hasClickAction()
                    Espresso.closeSoftKeyboard()
                    compose.onNodeWithTag("message-timeline").performScrollToNode(chip)
                    compose.onNode(chip).assertIsDisplayed().performClick()
                }
                openThumbReactors()
                compose.onNodeWithTag("reaction-details").assertExists()
                assertTrue(
                    graph.chat.state.value.messages
                        .single { it.id == parentId }
                        .reactions
                        .single { it.emoji == "👍" }
                        .reactors
                        .any { it.inboxId == owner.client.inboxId() },
                )
                back()
                until("reactors Back retains selected popover") {
                    graph.chat.state.value.overlay
                        ?.surface == MessageSurface.POPOVER
                }
                compose.onNodeWithTag("reaction-details").assertDoesNotExist()
                compose.onNodeWithTag("reaction-popover").assertExists()
                back()
                openThumbReactors()
                textAction("Remove my reaction").performClick()
                until("native removal reduced in chip") {
                    graph.chat.state.value.messages
                        .single { it.id == parentId }
                        .reactions
                        .none { it.emoji == "👍" }
                }
                assertTrue(
                    checkNotNull(owner.client.conversations.getMessageById(parentId)).reactions.any {
                        it.reaction.content == "👍" && it.reaction.action == ReactionAction.REMOVED
                    },
                )

                graph.sends.beforePublish = { throw IOException("Held publication after actual acceptance") }
                composer().performTextInput("Accepted queued retry")
                compose.onNodeWithContentDescription("Send message").performClick()
                until("accepted publication failure is visible") {
                    graph.chat.state.value.failure
                        ?.code == "send-publication"
                }
                val acceptedResult = checkNotNull(graph.chat.state.value.composer.result)
                assertEquals(chatId, acceptedResult.conversationId)
                assertTrue(acceptedResult.outcome is TextSendOutcome.Accepted)
                val acceptedId = (acceptedResult.outcome as TextSendOutcome.Accepted).messageId
                val acceptedDraft =
                    session.preferences.drafts(owner.key.profileId).single {
                        it.conversationKey == chatId && it.acceptedMessageId == acceptedId
                    }
                assertEquals(acceptedId, acceptedDraft.acceptedMessageId)
                graph.sends.beforePublish = {}
                compose
                    .onNode(
                        hasText("Retry") and hasClickAction() and
                            hasAnyAncestor(hasTestTag("publication-recovery")),
                    ).performClick()
                until("UI retry publishes only its accepted ID") {
                    owner.client.conversations
                        .getMessageById(acceptedId)
                        ?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                assertEquals(
                    listOf(acceptedId),
                    Conversation
                        .Group(group)
                        .messages(publishedSelection())
                        .filter { it.standardContent() == MessageContent.Text("Accepted queued retry") }
                        .map { it.id },
                )
                until("peer received the recovered ID") {
                    peerChat.messages(publishedSelection()).any { it.id == acceptedId }
                }
                assertEquals(
                    listOf(acceptedId),
                    peerChat
                        .messages(publishedSelection())
                        .filter { it.standardContent() == MessageContent.Text("Accepted queued retry") }
                        .map { it.id },
                )
                val inTimeline = hasAnyAncestor(hasTestTag("message-timeline"))
                compose.onAllNodes(hasText(" · Delivered", substring = true) and inTimeline).assertCountEquals(0)
                compose.onAllNodes(hasText(" · Read", substring = true) and inTimeline).assertCountEquals(0)
                assertEquals(
                    DeliveryStatus.PUBLISHED,
                    checkNotNull(owner.client.conversations.getMessageById(sentId)).deliveryStatus,
                )
                val ownText = hasText("Hello from this device") and hasClickAction() and inTimeline
                Espresso.closeSoftKeyboard()
                compose.waitForIdle()
                compose.onNodeWithTag("message-timeline").performScrollToNode(ownText)
                compose
                    .onNode(ownText)
                    .assertIsDisplayed()
                    .assert(hasText(" · Sent", substring = true))
                    .performClick()
                textAction("Delete").performClick()
                textAction("Delete message").performClick()
                until("actual own message deletion") {
                    owner.client.conversations
                        .getMessageById(sentId)
                        ?.toRow(owner.client.inboxId())
                        ?.deleted == true
                }
                screen("messages")
                compose.onNodeWithContentDescription("Conversation details").performClick()
                compose.onNodeWithTag("conversation-settings").performScrollToNode(hasText("Developer tools"))
                compose.onNodeWithText("Developer tools").assertIsDisplayed()
                screen("settings")
                compose.onNodeWithContentDescription("Back").performClick()
                previewBack()
                assertEquals(AppRoute.Chat(chatId), graph.destinations.state.value.route)
                commitBack()
                until("Back returns to actual Chats") { graph.destinations.state.value.route == AppRoute.Chats }

                val requestSender = second.conversations.createDm(owner.client.inboxId())
                val requestMessage = requestSender.sendText("A real message request")
                until("native incoming request exists") {
                    owner.client.conversations.getMessageById(requestMessage) != null
                }
                val requestId = checkNotNull(owner.client.conversations.getMessageById(requestMessage)).conversationId
                val request = checkNotNull(owner.client.conversations.getById(requestId))
                request.updateConsentState(ConsentState.UNKNOWN)
                until("native request listed") {
                    graph.conversations.state.value.requests
                        .any { it.id == requestId }
                }
                compose.onNode(hasText("Message requests", substring = true) and hasClickAction()).performClick()
                until("request list is active") {
                    val list = graph.conversations.state.value
                    graph.destinations.state.value.route == AppRoute.Requests &&
                        list.unknownTab && !list.busy &&
                        list.requests.any { it.id == requestId && it.preview == "A real message request" }
                }
                val requestRow = hasText("A real message request", substring = true) and hasClickAction()
                compose.onNode(hasScrollToIndexAction()).performScrollToNode(requestRow)
                compose.onNode(requestRow).assertIsDisplayed().performClick()
                until("actual request trust state") {
                    graph.chat.state.value.conversationId == requestId && graph.chat.state.value.unknown
                }
                val requestDraft = "Reply after explicit acceptance"
                compose.runOnIdle {
                    graph.chat.dispatch(ChatAction.ChangeDraft(requestDraft, requestDraft.length, requestDraft.length))
                }
                until("request retains its draft") { graph.chat.state.value.composer.text == requestDraft }
                compose
                    .onNode(hasText(requestDraft) and hasAnyAncestor(hasTestTag("message-composer")))
                    .assertIsNotEnabled()
                    .assertTextEquals(requestDraft)
                compose.onNodeWithContentDescription("Send message").assertIsNotEnabled().performTouchInput { click() }
                compose.onNodeWithContentDescription("Add attachment").assertDoesNotExist()
                compose.waitForIdle()
                assertEquals(ConsentState.UNKNOWN, conversationState(request).consentState)
                assertFalse(
                    "A request must not send the draft before Accept",
                    request.messages(publishedSelection().copy(deliveryStatus = null)).any {
                        it.standardContent() == MessageContent.Text(requestDraft)
                    },
                )
                assertEquals(requestDraft, graph.chat.state.value.composer.text)
                compose.onNode(hasText("A real message request") and hasClickAction()).performClick()
                compose.onNodeWithTag("reaction-popover").assertIsDisplayed()
                compose.onNodeWithText("👍").assertIsNotEnabled().performTouchInput { click() }
                textAction("Reply").assertIsNotEnabled()
                compose.onNodeWithContentDescription("More reactions").assertIsNotEnabled()
                textAction("Copy").assertIsEnabled().performClick()
                compose.waitForIdle()
                assertEquals(ConsentState.UNKNOWN, conversationState(request).consentState)
                assertTrue(
                    "A request must not add a native reaction before Accept",
                    checkNotNull(owner.client.conversations.getMessageById(requestMessage)).reactions.isEmpty(),
                )
                val retainedActionJobs =
                    compose.runOnIdle {
                        val parent = checkNotNull(owner.work.coroutineContext[Job])
                        val activeJobs = parent.children.toSet()
                        graph.chat.dispatch(ChatAction.React(requestMessage, "😮", false))
                        parent.children.filter { it !in activeJobs }.toList()
                    }
                withTimeout(30_000) { retainedActionJobs.joinAll() }
                assertEquals(ConsentState.UNKNOWN, conversationState(request).consentState)
                assertTrue(
                    "A retained reaction callback must not send before Accept",
                    checkNotNull(owner.client.conversations.getMessageById(requestMessage)).reactions.isEmpty(),
                )
                textAction("Accept").performClick()
                until(
                    "Accept commits native consent",
                ) { conversationState(request).consentState == ConsentState.ALLOWED }
                until("accepted request becomes a chat") { !graph.chat.state.value.unknown }
                composer().assertIsEnabled().assertTextEquals(requestDraft)
                compose.onNode(hasText("A real message request") and hasClickAction()).performClick()
                compose.onNodeWithText("👍").assertIsEnabled().performClick()
                until("accepted request permits a native reaction") {
                    checkNotNull(owner.client.conversations.getMessageById(requestMessage)).reactions.any {
                        it.reaction.content == "👍" && it.reaction.action == ReactionAction.ADDED
                    }
                }
                if (graph.chat.state.value.features.attachments) {
                    compose.onNodeWithContentDescription("Add attachment").assertIsEnabled()
                }
                compose.onNodeWithContentDescription("Send message").assertIsEnabled().performClick()
                until("accepted request publishes its preserved draft") {
                    request.messages(publishedSelection()).any {
                        it.standardContent() == MessageContent.Text(requestDraft)
                    }
                }
                assertEquals(ConsentState.ALLOWED, conversationState(request).consentState)
                queuedConsentAdmission(owner, request, requestMessage)
                retryDeleteConsentAdmission(owner, request)
                assertEquals(requestMessage, checkNotNull(owner.client.conversations.getMessageById(requestMessage)).id)
                back()
                until("accepted request returns to list") { graph.destinations.state.value.route == AppRoute.Requests }
                compose.onNode(hasText("Back to chats") and hasClickAction()).performClick()
                until("request acceptance retains Chats") { graph.destinations.state.value.route == AppRoute.Chats }
                compose.onNodeWithContentDescription("Settings").performClick()
                screen("app-settings")
                if (Build.VERSION.SDK_INT >= 33) {
                    val savedDisplay = readScreenDisplay(::checkedScreenShell)
                    val savedConfiguration = android.content.res.Configuration(compose.activity.resources.configuration)
                    val locales =
                        checkNotNull(
                            InstrumentationRegistry
                                .getInstrumentation()
                                .targetContext
                                .getSystemService(android.app.LocaleManager::class.java),
                        )
                    val savedLocales = locales.applicationLocales
                    val savedAppearance = graph.settings.state.value.appearance
                    val savedRoute = graph.destinations.state.value.route
                    var variantFailure: Throwable? = null

                    fun restore(step: () -> Unit) {
                        try {
                            step()
                        } catch (error: Throwable) {
                            if (variantFailure == null) {
                                variantFailure = error
                            } else if (variantFailure !== error) {
                                variantFailure?.addSuppressed(error)
                            }
                        }
                    }

                    fun actionBounds(description: String): androidx.compose.ui.geometry.Rect {
                        val action = compose.onNodeWithContentDescription(description).assertIsDisplayed()
                        val bounds = action.fetchSemanticsNode().touchBoundsInRoot
                        val minimum = 48f * compose.activity.resources.displayMetrics.density
                        assertTrue("$description is narrower than 48 dp", bounds.width >= minimum - 1)
                        assertTrue("$description is shorter than 48 dp", bounds.height >= minimum - 1)
                        return action.fetchSemanticsNode().boundsInRoot
                    }

                    fun assertRetainedDraft(
                        revision: Long,
                        text: String,
                    ) {
                        assertSame("A display change replaced the native session", owner, session.active.value)
                        assertEquals(AppRoute.Chat(requestId), graph.destinations.state.value.route)
                        assertEquals(text, graph.chat.state.value.composer.text)
                        assertEquals(revision, graph.chat.state.value.composer.revision)
                        composer().assertTextContains(text)
                    }

                    try {
                        val darkAction = hasText("Dark") and hasClickAction()
                        compose.onNodeWithTag("app-settings").performScrollToNode(darkAction)
                        compose.onNode(darkAction).assertIsDisplayed().performClick()
                        until("dark appearance is active") { graph.settings.state.value.appearance == Appearance.DARK }
                        val dark = compose.onNodeWithTag("back-preview-pane").captureToImage().toPixelMap()
                        assertTrue("The actual header did not use a dark surface", dark[2, 2].luminance() < 0.1f)
                        screen("ux-variant-dark")

                        compose.runOnIdle {
                            graph.conversations.dispatch(
                                org.xmtp.android.example.shared.feature.conversations.ConversationsAction
                                    .OpenConversation(requestId),
                            )
                        }
                        until("variant chat is active") {
                            graph.destinations.state.value.route == AppRoute.Chat(requestId) &&
                                graph.chat.state.value.conversationId == requestId &&
                                graph.chat.state.value.messages
                                    .any { it.id == requestMessage }
                        }
                        val variantText = "Draft kept across display changes"
                        composer().performTextReplacement(variantText)
                        val variantRevision = graph.chat.state.value.composer.revision
                        Espresso.closeSoftKeyboard()

                        checkedScreenShell("wm size 320x680")
                        checkedScreenShell("wm density 160")
                        checkedScreenShell("settings put system font_scale 2.0")
                        InstrumentationRegistry.getInstrumentation().runOnMainSync {
                            locales.applicationLocales = android.os.LocaleList.forLanguageTags("ar")
                        }
                        compose.waitUntil(30_000) {
                            val configuration = compose.activity.resources.configuration
                            configuration.screenWidthDp == 320 && configuration.fontScale == 2f &&
                                configuration.layoutDirection == android.view.View.LAYOUT_DIRECTION_RTL
                        }
                        assertRetainedDraft(variantRevision, variantText)
                        val rtlSend = actionBounds("Send message")
                        val rtlEditor = composer().assertIsDisplayed().fetchSemanticsNode().boundsInRoot
                        assertTrue(
                            "RTL did not move Send to the end of the composer",
                            rtlSend.right <= rtlEditor.left + 1,
                        )
                        screen("ux-variant-dark-rtl-200")

                        InstrumentationRegistry.getInstrumentation().runOnMainSync {
                            locales.applicationLocales = android.os.LocaleList.forLanguageTags("en")
                        }
                        checkedScreenShell("settings put system font_scale 1.0")
                        checkedScreenShell("wm size 760x400")
                        compose.waitUntil(30_000) {
                            val configuration = compose.activity.resources.configuration
                            configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE &&
                                configuration.fontScale == 1f &&
                                configuration.layoutDirection == android.view.View.LAYOUT_DIRECTION_LTR
                        }
                        assertRetainedDraft(variantRevision, variantText)
                        composer().performTouchInput { click() }
                        compose.waitUntil(30_000) {
                            compose.activity.window.decorView.rootWindowInsets
                                ?.isVisible(
                                    android.view.WindowInsets.Type
                                        .ime(),
                                ) == true
                        }
                        val send = actionBounds("Send message")
                        val editor = composer().assertIsDisplayed().fetchSemanticsNode().boundsInRoot
                        var imeTop = 0f
                        var contentTop = 0
                        compose.runOnIdle {
                            val decor = compose.activity.window.decorView
                            val decorLocation = IntArray(2)
                            decor.getLocationOnScreen(decorLocation)
                            val contentLocation = IntArray(2)
                            checkNotNull(compose.activity.findViewById<android.view.View>(android.R.id.content))
                                .getLocationOnScreen(contentLocation)
                            contentTop = contentLocation[1]
                            imeTop = decorLocation[1] + decor.height -
                                checkNotNull(
                                    decor.rootWindowInsets,
                                ).getInsets(
                                    android.view.WindowInsets.Type
                                        .ime(),
                                ).bottom
                                    .toFloat()
                        }
                        assertTrue("Send is behind the landscape keyboard", send.bottom + contentTop <= imeTop + 1)
                        assertTrue(
                            "The editor is behind the landscape keyboard",
                            editor.bottom + contentTop <= imeTop + 1,
                        )
                        assertRetainedDraft(variantRevision, variantText)
                        screen("ux-variant-landscape-ime", closeKeyboard = false)
                        Espresso.closeSoftKeyboard()

                        checkedScreenShell("wm size 1100x760")
                        compose.waitUntil(30_000) { compose.activity.resources.configuration.screenWidthDp >= 1000 }
                        assertRetainedDraft(variantRevision, variantText)
                        val sidebar = actionBounds("Settings")
                        val pane =
                            compose
                                .onNodeWithTag("back-preview-pane")
                                .assertIsDisplayed()
                                .fetchSemanticsNode()
                                .boundsInRoot
                        assertTrue("Chats and the active chat are not separate panes", sidebar.right <= pane.left + 1)
                        assertTrue(
                            "The active chat pane is too narrow",
                            pane.width >= 500f * compose.activity.resources.displayMetrics.density,
                        )
                        actionBounds("Send message")
                        screen("ux-variant-wide-chat")

                        restoreScreenDisplay(::checkedScreenShell, savedDisplay)
                        compose.waitUntil(30_000) {
                            val configuration = compose.activity.resources.configuration
                            configuration.screenWidthDp == savedConfiguration.screenWidthDp &&
                                configuration.fontScale == savedConfiguration.fontScale
                        }
                        assertRetainedDraft(variantRevision, variantText)
                        assertEquals(
                            requestMessage,
                            checkNotNull(owner.client.conversations.getMessageById(requestMessage)).id,
                        )
                        println(
                            "UI_VARIANT_PROOF dark=true rtl-320-200=true landscape-ime=true " +
                                "wide-chat=true native-owner-kept=true draft-kept=true",
                        )
                    } catch (error: Throwable) {
                        variantFailure = error
                    } finally {
                        restore { Espresso.closeSoftKeyboard() }
                        restore {
                            graph.settings.dispatch(
                                SettingsAction.SetAppearance(savedAppearance),
                            )
                        }
                        restore {
                            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                                locales.applicationLocales = savedLocales
                            }
                        }
                        restore { restoreScreenDisplay(::checkedScreenShell, savedDisplay) }
                        restore {
                            compose.waitUntil(30_000) {
                                val configuration = compose.activity.resources.configuration
                                configuration.screenWidthDp == savedConfiguration.screenWidthDp &&
                                    configuration.orientation == savedConfiguration.orientation &&
                                    configuration.fontScale == savedConfiguration.fontScale &&
                                    configuration.layoutDirection == savedConfiguration.layoutDirection &&
                                    locales.applicationLocales == savedLocales
                            }
                            assertEquals(savedDisplay, readScreenDisplay(::checkedScreenShell))
                            assertEquals(savedAppearance, graph.settings.state.value.appearance)
                        }
                        restore { graph.dispatch(ShellAction.Navigate(savedRoute)) }
                    }
                    variantFailure?.let { throw it }
                }
            } finally {
                releaseSend.complete(Unit)
                releaseHistory.complete(Unit)
                graph.chat.historyPageRead =
                    { chat, options, before, after -> chat.historyPage(options, before, after) }
                graph.chat.beforeQueuedAction = {}
                graph.sends.beforePublish = {}
                peerReader?.cancelAndJoin()
                withContext(NonCancellable) {
                    peer?.end()
                    if (session.active.value != null) session.deleteAccount()
                    session.signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}
