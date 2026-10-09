package org.xmtp.android.example.messenger

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.security.SecureRandom

class MessengerRaceInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(check: () -> Boolean) = withTimeout(30_000) { while (!check()) delay(20) }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
        val owner = checkNotNull(model.session.active.value)
        until { model.state.value.inbox == owner.client.inboxId() }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.lookupConversation = { owner, id -> owner.client.conversations.getById(id) }
            model.historyRead = { chat, options -> chat.messages(options) }
            model.historyCount = { chat, options -> chat.countMessages(options) }
            if (model.session.active.value != null) model.session.deleteAccount()
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    @Test fun suspendedOldOpenCannotReplaceNewChatOrSettings() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "A"))
                val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "B"))
                val entered = CompletableDeferred<Unit>()
                val returned = CompletableDeferred<Unit>()
                val completed = CompletableDeferred<Unit>()
                var attemptA: Long? = null
                model.onOpenStarted = { attempt, id -> if (id == a.id()) attemptA = attempt }
                model.onOpenAttemptFinished = { attempt, id ->
                    if (attempt == attemptA && id == a.id()) {
                        println(
                            "OPEN_PROOF attempt=$attempt old-operation-completed screen=${model.state.value.screen} chat=${model.state.value.conversationId}",
                        )
                        completed.complete(Unit)
                    }
                }
                model.lookupConversation = { current, id ->
                    val result = current.client.conversations.getById(id)
                    if (id == a.id()) {
                        entered.complete(Unit)
                        release.await()
                        returned.complete(Unit)
                    }
                    result
                }
                model.dispatch(MessengerAction.OpenConversation(a.id()))
                withTimeout(30_000) { entered.await() }
                println("OPEN_PROOF stage=old-lookup-blocked attempt=$attemptA")
                model.dispatch(MessengerAction.OpenConversation(b.id()))
                until { model.state.value.conversationId == b.id() }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                until { model.state.value.settings.title == "B" }
                release.complete(Unit)
                withTimeout(30_000) { returned.await() }
                withTimeout(30_000) { completed.await() }
                assertEquals(b.id(), model.state.value.conversationId)
                assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun restoredWindowUsesRawCoverageAndStopsAtUnretainedTie() =
        runBlocking {
            try {
                val owner = connect()
                // Isolate this history read from unrelated refresh signals.
                model.session.onInvalidated = {}
                model.session.onMessage = { _, _ -> }
                val group = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Restore"))
                val templateId = group.sendText("template")
                val template = checkNotNull(owner.client.conversations.getMessageById(templateId))
                var rawSize = 501

                fun records() =
                    (0 until rawSize).map { index ->
                        Message(
                            template.data.copy(
                                id = index.toString(16).padStart(64, '0'),
                                sentAt = Timestamp(10),
                                insertedAt = Timestamp(index.toLong()),
                            ),
                        )
                    }
                model.historyRead = { chat, options ->
                    if (chat.id() != group.id()) {
                        chat.messages(options)
                    } else {
                        records()
                            .filter { options.sentBefore == null || it.sentAt.ns < options.sentBefore!!.ns }
                            .take(checkNotNull(options.limit).toInt())
                            .filterIndexed { index, _ -> rawSize != 80 || index != 5 }
                    }
                }
                model.historyCount = { chat, options ->
                    if (chat.id() != group.id()) {
                        chat.countMessages(options)
                    } else {
                        records()
                            .count {
                                (options.sentBefore == null || it.sentAt.ns < options.sentBefore!!.ns) &&
                                    (options.sentAfter == null || it.sentAt.ns > options.sentAfter!!.ns)
                            }.toULong()
                    }
                }
                val saved = ScrollAnchor("0".repeat(64), 10, 7, false)
                model.session.preferences.saveAnchor(owner.key.profileId, group.id(), saved)
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until {
                    model.state.value.historyNotice
                        ?.contains("More history") == true
                }
                assertFalse(model.state.value.hasOlder)
                assertTrue(
                    model.state.value.messages
                        .isEmpty(),
                )
                model.dispatch(MessengerAction.LoadOlder)
                until {
                    model.state.value.historyNotice
                        ?.contains("More history") == true
                }
                assertTrue(
                    model.state.value.messages
                        .isEmpty(),
                )
                rawSize = 80
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until { model.state.value.messages.size == 79 }
                assertFalse(model.state.value.hasOlder)
                assertNull(model.state.value.historyNotice)
            } finally {
                cleanup()
            }
        }

    @Test fun refreshRereadsRetainedNativeWindowAfterMoreThanFiveHundredNewerRows() =
        runBlocking {
            try {
                val owner = connect()
                model.session.onInvalidated = {}
                model.session.onMessage = { _, _ -> }
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
                val saved = ScrollAnchor(anchor.id, anchor.sentAt.ns, 17, false)
                model.session.preferences.saveAnchor(owner.key.profileId, group.id(), saved)
                model.dispatch(MessengerAction.OpenConversation(group.id()))
                until {
                    model.state.value.messages
                        .any { it.id == anchor.id } &&
                        model.state.value.anchor
                            ?.offsetPx == 17
                }
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
                model.dispatch(MessengerAction.LoadOlder)
                until {
                    model.state.value.messages
                        .any { it.id == ids[0] }
                }
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
                model.session.onInvalidated = {}
                model.session.onMessage = { _, _ -> }
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
                    compose.onNodeWithText("React", useUnmergedTree = true).assertExists()
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
                    if (!expiry) compose.onNodeWithText("Close", useUnmergedTree = true).performClick()
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
