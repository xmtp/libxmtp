package org.xmtp.android.example.messenger

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

    private fun screen(name: String) {
        Espresso.closeSoftKeyboard()
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
                textAction("Create group").performScrollTo().performClick()
                until("created group timeline") {
                    graph.chat.state.value.conversationId != null && !graph.chat.state.value.busy
                }
                val chatId = checkNotNull(graph.chat.state.value.conversationId)
                val group = (checkNotNull(owner.client.conversations.getById(chatId)) as Conversation.Group).group
                assertEquals("Navy proof group", group.state().name)
                until("peer joined UI-created group") { second.conversations.getById(chatId) != null }
                val peerChat = checkNotNull(second.conversations.getById(chatId))
                val parentBody = "Quote first line\nHidden second line"
                val parentId = peerChat.sendText(parentBody)
                until("actual incoming text rendered") {
                    graph.chat.state.value.messages
                        .any { it.id == parentId }
                }

                fun selectParent() {
                    compose
                        .onNode(
                            hasText(parentBody) and hasClickAction() and !hasText("A reply from this device"),
                        ).performClick()
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

                composer().performTextInput("unsent draft")
                selectParent()
                textAction("Reply").performClick()
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
                back()
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
                back()
                compose.onNodeWithText("👍 1").performClick()
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
                compose.onNodeWithText("👍 1").performClick()
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
                val acceptedId =
                    checkNotNull(
                        session.preferences
                            .drafts(owner.key.profileId)
                            .single()
                            .acceptedMessageId,
                    )
                assertTrue(
                    graph.chat.state.value.composer.result
                        ?.outcome is TextSendOutcome.Accepted,
                )
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
                compose.onAllNodesWithText("Delivered", substring = false).assertCountEquals(0)
                compose.onAllNodesWithText("Read", substring = false).assertCountEquals(0)
                assertTrue(compose.onAllNodesWithText("Sent", substring = false).fetchSemanticsNodes().isNotEmpty())

                compose.onNodeWithText("Hello from this device").performClick()
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
                back()
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
                until("request list is active") { graph.destinations.state.value.route == AppRoute.Requests }
                compose.onNodeWithText("A real message request").performClick()
                until("actual request trust state") {
                    graph.chat.state.value.conversationId == requestId && graph.chat.state.value.unknown
                }
                textAction("Accept").performClick()
                until(
                    "Accept commits native consent",
                ) { conversationState(request).consentState == ConsentState.ALLOWED }
                until("accepted request becomes a chat") { !graph.chat.state.value.unknown }
                assertEquals(requestMessage, checkNotNull(owner.client.conversations.getMessageById(requestMessage)).id)
                back()
                until("accepted request returns to list") { graph.destinations.state.value.route == AppRoute.Requests }
                compose.onNode(hasText("Back to chats") and hasClickAction()).performClick()
                until("request acceptance retains Chats") { graph.destinations.state.value.route == AppRoute.Chats }
                compose.onNodeWithContentDescription("Settings").performClick()
                screen("app-settings")
            } finally {
                releaseSend.complete(Unit)
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
