package org.xmtp.android.example.messenger

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatAction
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class MessengerRaceInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: suspend () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            until(check)
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        until("Connect route after sign-out") { model.state.value.screen == Screen.START }
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
        val owner = checkNotNull(model.session.active.value)
        until("Chats projection after native connect") {
            model.state.value.inbox == owner.client.inboxId() && model.state.value.screen == Screen.CONVERSATIONS
        }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.chat.lookupConversation = { owner, id -> owner.client.conversations.getById(id) }
            model.chat.historyPageRead = { chat, options, before, after -> chat.historyPage(options, before, after) }
            model.chat.onOpenFinished = {}
            if (model.session.active.value != null) model.session.deleteAccount()
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun suspendedOldOpenCannotReplaceNewChatOrSettings() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            val releaseB = CompletableDeferred<Unit>()

            // Keep each existing deadline and name the wait that fails.
            suspend fun <T> atStage(
                stage: String,
                action: suspend () -> T,
            ): T =
                try {
                    withTimeout(30_000) { action() }
                } catch (error: TimeoutCancellationException) {
                    throw AssertionError("Stage: $stage", error)
                }

            try {
                val owner = connect()
                val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "A"))
                val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "B"))
                val aId = a.id()
                val bId = b.id()
                println("OPEN_PROOF stage=ids A=$aId B=$bId key=${owner.key}")
                val entered = CompletableDeferred<Unit>()
                val returned = CompletableDeferred<Unit>()
                val completed = CompletableDeferred<Unit>()
                var attemptA: Long? = null
                model.chat.onOpenStarted = { attempt, id ->
                    println("OPEN_PROOF stage=start id=$id attempt=$attempt token=${model.chat.screenToken()}")
                    if (id == aId) attemptA = attempt
                }
                model.chat.onOpenAttemptFinished = { attempt, id ->
                    if (attempt == attemptA && id == a.id()) {
                        println(
                            "OPEN_PROOF attempt=$attempt old-operation-completed screen=${model.state.value.screen} chat=${model.state.value.conversationId}",
                        )
                        completed.complete(Unit)
                    }
                }
                model.chat.lookupConversation = { current, id ->
                    val result = current.client.conversations.getById(id)
                    if (id == a.id()) {
                        entered.complete(Unit)
                        release.await()
                        returned.complete(Unit)
                    }
                    result
                }
                model.dispatch(MessengerAction.OpenConversation(a.id()))
                atStage("old A native lookup entry") { entered.await() }
                println("OPEN_PROOF stage=old-lookup-blocked attempt=$attemptA")
                model.dispatch(MessengerAction.OpenConversation(b.id()))
                try {
                    until { model.state.value.conversationId == bId }
                } catch (error: TimeoutCancellationException) {
                    val chatId = model.state.value.conversationId
                    val token = model.chat.screenToken()
                    val accepted = model.session.accepts(owner.key)
                    throw AssertionError("B open: chat=$chatId token=$token accepted=$accepted", error)
                }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                try {
                    until { model.state.value.settings.title == "B" }
                } catch (error: TimeoutCancellationException) {
                    val screen = model.state.value.screen
                    val title = model.state.value.settings.title
                    throw AssertionError("B settings: screen=$screen title=$title", error)
                }
                release.complete(Unit)
                atStage("released old A native lookup return") { returned.await() }
                atStage("old A open completion") { completed.await() }
                assertEquals(b.id(), model.state.value.conversationId)
                assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)

                model.chat.lookupConversation = { current, id -> current.client.conversations.getById(id) }
                val retainedA = a.sendText("Retained A while B opens")
                model.dispatch(MessengerAction.OpenConversation(aId))
                until("retained A native transcript") {
                    model.chat.state.value.messages
                        .any { it.id == retainedA } && !model.chat.state.value.busy
                }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("Chats route before held B lookup") { model.state.value.screen == Screen.CONVERSATIONS }
                val beforeA =
                    Conversation
                        .Group(
                            a,
                        ).messages(publishedSelection().copy(deliveryStatus = null))
                        .map { it.id }
                val beforeB =
                    Conversation
                        .Group(
                            b,
                        ).messages(publishedSelection().copy(deliveryStatus = null))
                        .map { it.id }
                val enteredB = CompletableDeferred<Unit>()
                model.chat.lookupConversation = { current, id ->
                    val found = current.client.conversations.getById(id)
                    if (id == bId) {
                        enteredB.complete(Unit)
                        releaseB.await()
                    }
                    found
                }
                model.dispatch(MessengerAction.OpenConversation(bId))
                atStage("held B native lookup entry") { enteredB.await() }
                until("loading B hides retained A") {
                    model.state.value.chatState
                        ?.let { it.busy && it.conversationId == bId } == true
                }
                compose.waitForIdle()
                compose.onNodeWithText("Retained A while B opens").assertDoesNotExist()
                val earlierCommands = checkNotNull(owner.work.coroutineContext[Job]).children.toSet()
                compose.runOnIdle { model.dispatch(MessengerAction.SendText("Reject stale destination send")) }
                val attemptedCommands =
                    checkNotNull(owner.work.coroutineContext[Job])
                        .children
                        .filter {
                            it !in
                                earlierCommands
                        }.toList()
                atStage("stale destination command completion") { attemptedCommands.forEach { it.join() } }
                assertEquals(
                    beforeA,
                    Conversation.Group(a).messages(publishedSelection().copy(deliveryStatus = null)).map { it.id },
                )
                assertEquals(
                    beforeB,
                    Conversation.Group(b).messages(publishedSelection().copy(deliveryStatus = null)).map { it.id },
                )
                releaseB.complete(Unit)
                until("released B native transcript") {
                    model.chat.state.value.conversationId == bId &&
                        !model.chat.state.value.busy
                }
                model.chat.lookupConversation = { current, id -> current.client.conversations.getById(id) }
                compose.onNodeWithText("Message").performTextReplacement("Explicit B send")
                compose.onNodeWithText("Send").performClick()
                until("explicit B native send") {
                    Conversation.Group(b).messages(publishedSelection()).any {
                        it.standardContent() ==
                            MessageContent.Text("Explicit B send")
                    }
                }
                assertEquals(
                    beforeA,
                    Conversation.Group(a).messages(publishedSelection().copy(deliveryStatus = null)).map { it.id },
                )
                val sentB =
                    Conversation.Group(b).messages(publishedSelection()).filter {
                        it.standardContent() ==
                            MessageContent.Text("Explicit B send")
                    }
                assertEquals(1, sentB.size)
                assertEquals(bId, sentB.single().conversationId)
                println("OPEN_PROOF stage=loading-B-rejects-stale-A-send-explicit-B-send-keeps-origin")
            } finally {
                release.complete(Unit)
                releaseB.complete(Unit)
                cleanup()
            }
        }

    @Test fun sdkRawContinuationReachesReadableRowsAfterAnEmptyPrefix() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Raw coverage"),
                    )
                val ids = (0 until 250).map { group.sendText("Raw $it", SendOptions(optimistic = true)) }
                group.publishMessages()
                assertEquals(250uL, group.countMessages(publishedSelection()))
                val missing = ids.takeLast(200).toSet()
                val calls =
                    java.util.concurrent.atomic
                        .AtomicInteger()
                model.chat.historyPageRead = { chat, options, before, after ->
                    val page = chat.historyPage(options, before, after)
                    if (chat.id() != group.id() || options.limit != 50u) {
                        page
                    } else {
                        calls.incrementAndGet()
                        val readable = page.messages.filter { it.id !in missing }
                        val skipped = (page.messages.size - readable.size).toUInt()
                        page.copy(messages = readable, skippedCount = page.skippedCount + skipped)
                    }
                }
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                assertEquals(4, calls.get())
                assertTrue(
                    model.state.value.messages
                        .isEmpty(),
                )
                assertTrue(model.state.value.hasOlder)
                assertTrue(
                    model.state.value.historyNotice
                        ?.contains("cannot be read") == true,
                )
                println("SDK_PAGE_PROOF stage=empty-prefix-raw-continuation")
                model.dispatch(MessengerAction.LoadOlder)
                until("readable page after raw prefix") {
                    model.state.value.messages.size == 50 || !model.state.value.hasOlder
                }
                assertEquals(
                    ids.take(50).asReversed(),
                    model.state.value.messages
                        .map { it.id },
                )
                assertFalse(model.state.value.hasOlder)
                assertEquals(5, calls.get())
                println("SDK_PAGE_PROOF stage=readable-page-after-empty-prefix")
            } finally {
                cleanup()
            }
        }

    @Test fun nativeSdkPagesReachFiveHundredAndOneRowsWithBoundedCache() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "SDK pages"),
                    )
                val ids = (0 until 501).map { group.sendText("Page $it", SendOptions(optimistic = true)) }
                group.publishMessages()
                assertEquals(501uL, group.countMessages(publishedSelection()))
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                assertEquals(
                    ids.takeLast(50).asReversed(),
                    model.state.value.messages
                        .map { it.id },
                )
                val seen =
                    model.state.value.messages
                        .map { it.id }
                        .toMutableSet()
                var oldest =
                    model.state.value.messages
                        .last()
                        .id
                while (model.state.value.hasOlder) {
                    val row =
                        model.state.value.messages
                            .last()
                    model.dispatch(MessengerAction.Viewport(ScrollAnchor(row.id, row.sentAtNs, 0, false), false))
                    until("SDK viewport position") {
                        model.state.value.anchor
                            ?.messageId == row.id
                    }
                    model.dispatch(MessengerAction.LoadOlder)
                    until("next real SDK page") {
                        model.state.value.messages
                            .lastOrNull()
                            ?.id != oldest || !model.state.value.hasOlder
                    }
                    oldest =
                        model.state.value.messages
                            .last()
                            .id
                    seen.addAll(
                        model.state.value.messages
                            .map { it.id },
                    )
                    assertTrue(model.state.value.messages.size <= 500)
                    assertNull(model.state.value.historyNotice)
                }
                assertEquals(ids.toSet(), seen)
                assertEquals(501, seen.size)
                assertEquals(ids.first(), oldest)
                println("SDK_PAGE_PROOF stage=real-sdk-501-no-gaps-cache-bound")
            } finally {
                cleanup()
            }
        }

    @Test fun deletedNativeAnchorUsesItsRetainedTupleAndTheNextNewerRow() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Deleted boundary"),
                    )
                val ids = (0 until 160).map { group.sendText("Anchor $it", SendOptions(optimistic = true)) }
                group.publishMessages()
                assertEquals(160uL, group.countMessages(publishedSelection()))
                val anchor = checkNotNull(owner.client.conversations.getMessageById(ids[79]))
                val saved = ScrollAnchor(anchor.id, anchor.sentAt.ns, 23, false, checkNotNull(anchor.deliveryCursor))
                model.session.preferences.saveAnchor(owner.key.profileId, group.id(), saved)
                owner.client.conversations.deleteMessageLocally(anchor.id)
                assertNull(owner.client.conversations.getMessageById(anchor.id))
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                val restored = checkNotNull(model.state.value.anchor)
                assertEquals(ids[80], restored.messageId)
                assertEquals(0, restored.offsetPx)
                assertFalse(restored.wasAtNewest)
                assertNotNull(restored.deliveryCursor)
                assertEquals("Position changed", model.state.value.historyNotice)
                assertFalse(
                    model.state.value.messages
                        .any { it.id == anchor.id },
                )
                assertTrue(
                    model.state.value.messages
                        .any { it.id == ids[78] },
                )
                assertTrue(
                    model.state.value.messages
                        .any { it.id == ids[80] },
                )
                println("SDK_PAGE_PROOF stage=deleted-sdk-boundary-next-newer")
            } finally {
                cleanup()
            }
        }

    @Test fun deletedSdkPlaceholdersCannotBlockNewestAnchorFallback() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Deleted placeholders"),
                    )
                val ids = (0 until 160).map { group.sendText("Placeholder $it", SendOptions(optimistic = true)) }
                group.publishMessages()
                assertEquals(160uL, group.countMessages(publishedSelection()))
                val anchor = checkNotNull(owner.client.conversations.getMessageById(ids[79]))
                val saved = ScrollAnchor(anchor.id, anchor.sentAt.ns, 23, false, checkNotNull(anchor.deliveryCursor))
                val position = MessageHistoryPosition(anchor.sentAt, checkNotNull(anchor.deliveryCursor))
                for (id in ids.subList(30, 130)) group.deleteMessage(id)
                group.publishMessages()
                val newer =
                    group.messageHistoryPage(
                        publishedSelection().copy(limit = 50u, direction = MessageOrder.ASCENDING),
                        null,
                        position,
                    )
                val older = group.messageHistoryPage(publishedSelection().copy(limit = 50u), newer.firstPosition, null)
                for (page in listOf(newer, older)) {
                    assertEquals(50, page.messages.size)
                    assertTrue(
                        page.messages.all {
                            (it.content as? SDKMessageContent.Standard)?.value is MessageContent.DeletedMessage
                        },
                    )
                    assertNotNull(page.firstPosition)
                    assertNotNull(page.lastPosition)
                    assertEquals(0u, page.skippedCount)
                }
                model.session.preferences.saveAnchor(owner.key.profileId, group.id(), saved)
                val reads =
                    java.util.concurrent.atomic
                        .AtomicInteger()
                model.chat.historyPageRead = { chat, options, before, after ->
                    if (chat.id() == group.id() && options.limit == 50u) reads.incrementAndGet()
                    chat.historyPage(options, before, after)
                }
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                assertEquals("Native deleted placeholders require a newest-page read", 3, reads.get())
                val restored = checkNotNull(model.state.value.anchor)
                assertEquals(ids.last(), restored.messageId)
                assertEquals(0, restored.offsetPx)
                assertTrue(restored.wasAtNewest)
                assertEquals("Position changed", model.state.value.historyNotice)
                assertEquals(
                    ids.takeLast(50).asReversed(),
                    model.state.value.messages
                        .map { it.id },
                )
                assertTrue(
                    model.state.value.messages
                        .filter { it.id in ids.subList(30, 130) }
                        .all { it.deleted },
                )
                assertTrue(
                    model.state.value.messages
                        .none { it.id == anchor.id },
                )
                println("SDK_PAGE_PROOF stage=decoded-deletion-placeholder-newest-fallback")
            } finally {
                cleanup()
            }
        }

    @Test fun anchoredRefreshPreservesEveryLoadedNativeMessage() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Loaded range"),
                    )
                val ids = (0 until 200).map { group.sendText("Loaded $it", SendOptions(optimistic = true)) }
                group.publishMessages()
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                while (model.state.value.hasOlder) {
                    val size = model.state.value.messages.size
                    model.dispatch(MessengerAction.LoadOlder)
                    until { model.state.value.messages.size > size || !model.state.value.hasOlder }
                }
                assertEquals(
                    ids.asReversed(),
                    model.state.value.messages
                        .map { it.id },
                )
                val anchor = checkNotNull(owner.client.conversations.getMessageById(ids[100]))
                val saved = ScrollAnchor(anchor.id, anchor.sentAt.ns, 17, false, anchor.deliveryCursor)
                model.dispatch(MessengerAction.Viewport(saved, false))
                until { model.state.value.anchor == saved }
                group.deleteMessage(ids[110])
                group.publishMessages()
                model.dispatch(MessengerAction.Refresh)
                until {
                    model.state.value.messages
                        .any { it.id == ids[110] && it.deleted }
                }
                assertEquals(
                    "An anchored refresh must keep both loaded sides",
                    ids.asReversed(),
                    model.state.value.messages
                        .map { it.id },
                )
                assertEquals(saved, model.state.value.anchor)
                assertFalse(model.state.value.hasOlder)
                println("REFRESH_PROOF stage=anchored-multiple-pages-retained count=200")
            } finally {
                cleanup()
            }
        }

    @Test fun refreshRereadsRetainedNativeWindowAfterMoreThanFiveHundredNewerRows() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Old window"))
                val ids =
                    withTimeout(120_000) {
                        (1..650).map { index ->
                            group.sendText("History $index").also { delay(1) }
                        }
                    }
                val native = group.messages(publishedSelection().copy(limit = 651u))
                assertEquals(650, native.size)
                assertEquals(650, native.map { it.sentAt.ns }.toSet().size)
                val anchor = checkNotNull(owner.client.conversations.getMessageById(ids[99]))
                val removed = ids[89]
                val saved = ScrollAnchor(anchor.id, anchor.sentAt.ns, 17, false, anchor.deliveryCursor)
                model.session.preferences.saveAnchor(owner.key.profileId, group.id(), saved)
                val opened = CompletableDeferred<Unit>()
                model.chat.onOpenFinished = { if (it == group.id()) opened.complete(Unit) }
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                withTimeout(30_000) { opened.await() }
                assertEquals("Retained native SDK anchor", saved, model.state.value.anchor)
                assertTrue(
                    model.state.value.messages
                        .any { it.id == removed && !it.deleted },
                )
                println("REFRESH_PROOF retained-anchor=${anchor.id} newer-count=550")
                group.deleteMessage(removed)
                model.dispatch(MessengerAction.Refresh)
                until {
                    model.state.value.messages
                        .any { it.id == removed && it.deleted } ||
                        model.state.value.messages
                            .any { it.id == ids.last() }
                }
                val stillReadable =
                    model.state.value.messages
                        .any { it.id == removed && !it.deleted }
                println("REFRESH_PROOF completed-anchor=${model.state.value.anchor} removed-readable=$stillReadable")
                assertEquals(saved, model.state.value.anchor)
                assertTrue(
                    model.state.value.messages
                        .any { it.id == removed && it.deleted },
                )
                assertTrue(
                    model.state.value.messages
                        .any { it.id == anchor.id },
                )
                assertTrue(model.state.value.messages.size <= 500)
                assertTrue(model.state.value.hasOlder)
                assertNull(model.state.value.historyNotice)
                println("REFRESH_PROOF stage=retained-anchor-and-current-removal")
                while (model.state.value.hasOlder) {
                    val oldest =
                        model.state.value.messages
                            .last()
                            .id
                    model.dispatch(MessengerAction.LoadOlder)
                    until {
                        model.state.value.messages
                            .lastOrNull()
                            ?.id != oldest || !model.state.value.hasOlder
                    }
                }
                assertTrue(
                    model.state.value.messages
                        .any { it.id == ids[0] },
                )
                assertTrue(
                    model.state.value.messages
                        .any { it.id == anchor.id },
                )
                assertFalse(model.state.value.hasOlder)
                assertTrue(model.state.value.messages.size <= 500)
                println("REFRESH_PROOF stage=older-cursor-complete")
            } finally {
                cleanup()
            }
        }

    @Test fun selectedDialogAndReplyClearAfterNativeDeletionAndExpiry() =
        runBlocking {
            try {
                val owner = connect()
                graphRule.observers.pauseAutomaticProjection()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Removed text"),
                    )
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until { model.state.value.conversationId == group.id() }
                for (expiry in listOf(false, true)) {
                    if (expiry) {
                        group.updateDisappearingSettings(
                            DisappearingSettings(Timestamp(System.currentTimeMillis() * 1_000_000), 5_000_000_000L),
                        )
                    }
                    val text = if (expiry) "Expiring private content" else "Deleted private content"
                    val id = group.sendText(text)
                    model.dispatch(MessengerAction.Refresh)
                    until {
                        model.state.value.messages
                            .any { it.id == id }
                    }
                    compose.waitForIdle()
                    compose.onAllNodesWithText(text).onFirst().performClick()
                    compose.onNodeWithText("Reply", useUnmergedTree = true).performClick()
                    until { model.state.value.replyTo == id }
                    assertEquals(text, model.state.value.replyPreview)
                    compose.waitForIdle()
                    compose.onAllNodesWithText(text).onFirst().performClick()
                    compose.onNodeWithContentDescription("More reactions", useUnmergedTree = true).assertExists()
                    if (expiry) {
                        withTimeout(30_000) {
                            while (owner.client.conversations.getMessageById(id) != null) delay(50)
                        }
                    } else {
                        group.deleteMessage(id)
                    }
                    model.dispatch(MessengerAction.Refresh)
                    until {
                        model.state.value.messages
                            .none { it.id == id && !it.deleted }
                    }
                    println("REMOVAL_PROOF refreshed-reply=${model.state.value.replyPreview}")
                    assertNull(model.state.value.replyPreview)
                    assertNull(model.state.value.replyTo)
                    compose.waitForIdle()
                    compose.onAllNodesWithText(text, substring = true).assertCountEquals(0)
                    println("REMOVAL_PROOF stage=${if (expiry) "expired" else "deleted"}-dialog-and-reply-current")
                    compose.onNodeWithContentDescription("More reactions", useUnmergedTree = true).assertDoesNotExist()
                }
            } finally {
                cleanup()
            }
        }

    @Test fun readingRealIncomingMessageRefreshesBadgeBeforeBack() =
        runBlocking {
            var peer: SDKClient? = null
            var reader: Job? = null
            try {
                val owner = connect()
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
                reader = launch { second.conversations.streamAllMessages().collect { } }
                val remote = second.conversations.createDm(owner.client.inboxId())
                val id = remote.sendText("Read this message")
                val stored =
                    withTimeout(30_000) {
                        var message: Message? = null
                        while (message == null) {
                            message = owner.client.conversations.getMessageById(id)
                            delay(20)
                        }
                        message
                    }
                val chat = checkNotNull(owner.client.conversations.getById(stored.conversationId))
                chat.updateConsentState(ConsentState.ALLOWED)
                model.dispatch(MessengerAction.Refresh)
                until {
                    model.state.value.conversations
                        .any { it.id == chat.id() && it.unread == "1" }
                }
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                until {
                    model.state.value.messages
                        .any { it.id == id }
                }
                model.foreground(true)
                model.dispatch(MessengerAction.Viewport(ScrollAnchor(id, stored.sentAt.ns, 0, true), true))
                until {
                    model.state.value.conversations
                        .any { it.id == chat.id() && it.unread == "0" }
                }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                assertEquals(
                    "0",
                    model.state.value.conversations
                        .single { it.id == chat.id() }
                        .unread,
                )
            } finally {
                reader?.cancelAndJoin()
                withContext(NonCancellable) { peer?.end() }
                cleanup()
            }
        }
}
