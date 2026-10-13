package org.xmtp.android.example.messenger

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Semaphore
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.data.xmtp.ConversationQueries
import org.xmtp.android.example.messenger.data.xmtp.XmtpQueries
import org.xmtp.android.example.messenger.feature.conversations.CreateConversationController
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.conversations.CreateConversationAction
import org.xmtp.android.example.shared.feature.conversations.RequestAction
import org.xmtp.android.example.shared.feature.conversations.RequestDecision
import uniffi.xmtp_sdk.*
import java.io.File
import java.nio.file.Files
import java.security.SecureRandom
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MessengerReviewInstrumentedTest {
    private val graphRule = TestAppGraphRule()
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)
    private val model get() = ViewModelProvider(compose.activity)[MessengerViewModel::class.java]

    private suspend fun until(
        stage: String,
        check: suspend () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (
            error: TimeoutCancellationException,
        ) {
            throw AssertionError("Stage did not finish: $stage", error)
        }
    }

    private suspend fun connect(): ActiveSession {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        model.session.signOut()
        until("signed-out UI") { model.state.value.screen == Screen.START }
        model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
        val owner = checkNotNull(model.session.active.value)
        until("connected UI") {
            model.state.value.inbox == owner.client.inboxId() && model.state.value.screen == Screen.CONVERSATIONS
        }
        model.foreground(false)
        return owner
    }

    private suspend fun cleanup() =
        withContext(NonCancellable) {
            model.session.preferences.beforePositionCommit = {}
            model.session.preferences.positionCommitFinished = { _, _ -> }
            model.conversations.writeConsent = { chat, value -> chat.updateConsentState(value) }
            model.conversations.onConsentFinished = {}
            model.chat.beforeQueuedAction = {}
            model.projection.beforeActiveRefresh = {}
            model.group.beforeGroupWrite = { _, _ -> }
            model.group.onGroupActionFinished = {}
            model.conversations.beforeConsentDecision = {}
            model.chat.onQueuedActionFinished = {}
            model.chat.actionMessageRead = { owner, id -> owner.client.conversations.getMessageById(id) }
            model.chat.historyPageRead = { chat, options, before, after -> chat.historyPage(options, before, after) }
            model.sends.messageRead = { client, id -> client.conversations.getMessageById(id) }
            graphRule.observers.onSessionInvalidated = {}
            graphRule.observers.beforeUnregister = {}
            if (model.session.active.value != null ||
                model.session.preferences.reset() != null
            ) {
                model.session.deleteAccount()
            }
            model.session.signOut()
            AndroidStreamLifecycle.enabled = true
        }

    private suspend fun openChat(id: String) {
        model.dispatch(MessengerAction.OpenConversation(id))
        until("open $id") { model.state.value.conversationId == id && model.state.value.screen == Screen.TIMELINE }
    }

    private suspend fun openDetails(id: String) {
        model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
        until("details owner $id") {
            model.state.value.screen == Screen.CONVERSATION_SETTINGS &&
                model.group.state.value.conversationId == id && !model.group.state.value.busy
        }
    }

    @Test fun closedOwnerDuringActiveProjectionDoesNotTerminateTheUiCollector() =
        runBlocking {
            val entered = CompletableDeferred<ActiveSession>()
            val finished = CompletableDeferred<Unit>()
            try {
                model.session.signOut()
                until("signed-out UI before owner refresh") { model.state.value.screen == Screen.START }
                model.projection.beforeActiveRefresh = { owner ->
                    entered.complete(owner)
                    try {
                        awaitCancellation()
                    } finally {
                        withContext(NonCancellable) {
                            finished.complete(Unit)
                            owner.client.conversations.list()
                        }
                    }
                }
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                until("native owner entered active projection") { entered.isCompleted }
                val owner = entered.await()
                model.session.signOut()
                until("cancelled owner projection finished before client close") { finished.isCompleted }
                until("closed owner returns to Start") { model.state.value.screen == Screen.START }
                assertFalse(model.session.accepts(owner.key))
                assertTrue(
                    runCatching { owner.client.conversations.list() }.exceptionOrNull() is XmtpException.ClientClosed,
                )
                model.projection.beforeActiveRefresh = {}
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val next = checkNotNull(model.session.active.value)
                until("collector handles next owner") { model.state.value.inbox == next.client.inboxId() }
                assertNull(model.state.value.error)
            } finally {
                cleanup()
            }
        }

    @Test fun nativeMarkdownIsVisibleAndKeepsItsReplyQuote() =
        runBlocking {
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Markdown"),
                    )
                val text = "**First line**\nSecond line"
                assertTrue(publishedSelection().contentTypes!!.contains(MarkdownCodec().type))
                val parentId = group.sendMarkdown(text)
                val chat = Conversation.Group(group)
                until("published Markdown") { chat.messages(publishedSelection()).any { it.id == parentId } }
                assertEquals(1uL, chat.countMessages(incomingSelection("0".repeat(64))))
                assertEquals(0uL, chat.countMessages(incomingSelection(owner.client.inboxId())))
                model.chat.dispatch(ChatAction.Refresh)
                until("Markdown conversation preview") {
                    model.state.value.conversations
                        .any { it.id == group.id() && it.preview == text }
                }
                openChat(group.id())
                until("Markdown timeline row") {
                    model.state.value.messages
                        .any { it.id == parentId && it.text == text }
                }
                model.dispatch(MessengerAction.Reply(parentId))
                assertEquals("**First line**", model.state.value.replyPreview)
                val parent = checkNotNull(owner.client.conversations.getMessageById(parentId))
                val replyId = parent.reply("Answer")
                until("native Markdown reply parent") {
                    owner.client.conversations
                        .getMessageById(replyId)
                        ?.inReplyTo != null
                }
                val reply = checkNotNull(owner.client.conversations.getMessageById(replyId))
                assertTrue(reply.inReplyTo?.content is MessageBody.Markdown)
                assertEquals(text, reply.toRow(owner.client.inboxId()).reply)
                model.chat.dispatch(ChatAction.Refresh)
                until("Markdown reply quote in timeline") {
                    model.state.value.messages
                        .any { it.id == replyId && it.reply == text }
                }
            } finally {
                cleanup()
            }
        }

    @Test fun pendingRecoveryPagesExposeOlderNativeIdsAndRetryWithoutDuplicateSend() =
        runBlocking {
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Pending pages"),
                    )
                val ids =
                    (0 until 80).map { index ->
                        group.sendText("Pending $index", SendOptions(optimistic = true)).also { delay(2) }
                    }
                val chat = Conversation.Group(group)
                assertEquals(
                    80uL,
                    chat.countMessages(
                        publishedSelection().copy(deliveryStatus = DeliveryStatus.UNPUBLISHED),
                    ),
                )
                assertEquals(0uL, chat.countMessages(incomingSelection("0".repeat(64))))
                openChat(group.id())
                until("fifty newest pending rows") {
                    model.state.value.messages
                        .count { it.status == "Queued" } == 50 &&
                        model.state.value.hasOlderRecovery
                }
                assertFalse(
                    model.state.value.messages
                        .any { it.id == ids.first() },
                )
                compose.onNodeWithText("Older pending messages").performClick()
                until("oldest SDK pending ID exposed") {
                    model.state.value.messages
                        .any { it.id == ids.first() } && !model.state.value.hasOlderRecovery
                }
                assertFalse(model.state.value.recoveryAtNewest)
                assertTrue(model.state.value.messages.size <= 50)
                val refreshed = CompletableDeferred<Unit>()
                model.chat.onQueuedActionFinished = { if (it == ChatAction.Refresh) refreshed.complete(Unit) }
                model.chat.dispatch(ChatAction.Refresh)
                until("selected pending window refresh completed") { refreshed.isCompleted }
                assertTrue(
                    model.state.value.messages
                        .any { it.id == ids.first() },
                )
                val finished = CompletableDeferred<Unit>()
                val retry = MessengerAction.RetrySend(ids.first())
                model.chat.onQueuedActionFinished = { if (it.matchesInput(retry)) finished.complete(Unit) }
                model.dispatch(retry)
                withTimeout(30_000) { finished.await() }
                until("oldest ID published by actual retry") {
                    owner.client.conversations
                        .getMessageById(ids.first())
                        ?.deliveryStatus == DeliveryStatus.PUBLISHED
                }
                assertEquals(80uL, chat.countMessages(publishedSelection().copy(deliveryStatus = null)))
                assertEquals(
                    80,
                    chat
                        .messages(publishedSelection().copy(deliveryStatus = null, limit = 100u))
                        .map { it.id }
                        .toSet()
                        .size,
                )
                model.dispatch(MessengerAction.LatestRecovery)
                until("pending newest window restored") { model.state.value.recoveryAtNewest }
            } finally {
                cleanup()
            }
        }

    @Test fun signOutCancelsStalledGroupWriteAfterActualFirstCommit() =
        runBlocking {
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val waitEnded = CompletableDeferred<Unit>()
            var stop: Deferred<Unit>? = null
            try {
                val owner = connect()
                val inbox = owner.client.inboxId()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Original", description = "Keep description"),
                    )
                val groupId = group.id()
                openChat(groupId)
                openDetails(groupId)
                val action = MessengerAction.UpdateGroup("First native commit", "Must not commit")
                model.group.beforeGroupWrite = { current, index ->
                    if (current.matchesInput(action) && index == 1) {
                        entered.complete(Unit)
                        try {
                            release.await()
                        } finally {
                            waitEnded.complete(Unit)
                        }
                    }
                }
                model.dispatch(action)
                withTimeout(30_000) { entered.await() }
                assertEquals("First native commit", group.state().name)
                assertEquals("Keep description", group.state().description)
                stop = async(Dispatchers.IO) { model.session.signOut() }
                assertEquals(
                    "Sign-out must cancel stalled group work without releasing its barrier",
                    "completed",
                    withTimeoutOrNull(3_000) {
                        stop.await()
                        "completed"
                    },
                )
                assertFalse(release.isCompleted)
                assertTrue(waitEnded.isCompleted)
                assertNull(model.session.active.value)
                assertFalse(model.session.preferences.signedIn())
                assertTrue(
                    runCatching { owner.client.conversations.listGroups(null) }
                        .exceptionOrNull() is XmtpException.ClientClosed,
                )
                model.group.beforeGroupWrite = { _, _ -> }
                model.group.onGroupActionFinished = {}
                model.conversations.beforeConsentDecision = {}
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, null, false)
                val reopened = checkNotNull(model.session.active.value)
                assertEquals(inbox, reopened.client.inboxId())
                val stored = (reopened.client.conversations.getById(groupId) as Conversation.Group).group
                assertEquals("First native commit", stored.state().name)
                assertEquals("Keep description", stored.state().description)
                println("GROUP_STOP_PROOF first_native_commit=true blocked_second_cancelled=true client_closed=true")
            } finally {
                release.complete(Unit)
                withContext(NonCancellable) {
                    stop?.join()
                    cleanup()
                }
            }
        }

    @Test fun admittedGroupWritesFinishOnOriginAfterNavigation() =
        runBlocking {
            val releases = mutableListOf<CompletableDeferred<Unit>>()
            try {
                val owner = connect()
                val a =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Origin", description = "Original"),
                    )
                val b =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Other", description = "Other description"),
                    )
                val actions =
                    listOf(
                        MessengerAction.UpdateGroup("Completed name", "Completed description"),
                        MessengerAction.SetPreset(true),
                    )
                for (action in actions) {
                    openChat(a.id())
                    openDetails(a.id())
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    val finished = CompletableDeferred<Unit>()
                    model.group.beforeGroupWrite = { current, index ->
                        if (current.matchesInput(action) && index == 1) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.group.onGroupActionFinished =
                        { current -> if (current.matchesInput(action)) finished.complete(Unit) }
                    model.dispatch(action)
                    withTimeout(30_000) { entered.await() }
                    if (action is MessengerAction.UpdateGroup) {
                        assertEquals("Completed name", a.state().name)
                        assertEquals("Original", a.state().description)
                    } else {
                        assertNotEquals(GroupPolicyType.ALL_MEMBERS, a.state().permissions.policyType)
                    }
                    println("GROUP_ACTION_PROOF stage=first-native-write-committed action=$action")
                    openChat(b.id())
                    model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                    until("other settings") { model.state.value.settings.title == "Other" }
                    release.complete(Unit)
                    withTimeout(30_000) { finished.await() }
                    if (action is MessengerAction.UpdateGroup) {
                        assertEquals("Completed name", a.state().name)
                        assertEquals("Completed description", a.state().description)
                    } else {
                        assertEquals(GroupPolicyType.ADMIN_ONLY, a.state().permissions.policyType)
                    }
                    assertEquals("Other", b.state().name)
                    assertEquals("Other description", b.state().description)
                    assertEquals(GroupPolicyType.ALL_MEMBERS, b.state().permissions.policyType)
                    assertEquals(b.id(), model.state.value.conversationId)
                    assertEquals("Other", model.state.value.settings.title)
                    assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)
                    assertNull(model.state.value.error)
                    println("GROUP_ACTION_PROOF stage=remaining-native-writes-finished-on-origin action=$action")
                }
            } finally {
                releases.forEach { it.complete(Unit) }
                cleanup()
            }
        }

    @Test fun queuedActionsCannotMutateEitherChatAfterSwitch() =
        runBlocking {
            val releases = mutableListOf<CompletableDeferred<Unit>>()
            try {
                val owner = connect()
                val a =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Origin A", description = "Original"),
                    )
                val b =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Current B", description = "Original"),
                    )
                a.updateConsentState(ConsentState.ALLOWED)
                b.updateConsentState(ConsentState.ALLOWED)
                val target = a.sendText("Origin parent")
                val pending = a.sendText("Pending origin", SendOptions(optimistic = true))
                val bParent = b.sendText("Current parent")
                val beforeA =
                    Conversation
                        .Group(a)
                        .messages()
                        .map { it.id }
                        .toSet()
                val beforeB =
                    Conversation
                        .Group(b)
                        .messages()
                        .map { it.id }
                        .toSet()
                val actions =
                    listOf(
                        MessengerAction.SendText("Queued origin reply"),
                        MessengerAction.Consent(false),
                        MessengerAction.UpdateGroup("Wrong name", "Wrong description"),
                        MessengerAction.RetrySend(pending),
                        MessengerAction.DeleteMessage(target),
                        MessengerAction.React(target, "👍", false),
                        MessengerAction.SetDisappearing(60),
                        MessengerAction.SetPreset(true),
                    )
                for (action in actions) {
                    openChat(a.id())
                    if (action.isGroupInput()) openDetails(a.id())
                    until("origin parent visible") {
                        model.state.value.messages
                            .any { it.id == target }
                    }
                    model.dispatch(MessengerAction.Reply(target))
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    val finished = CompletableDeferred<Unit>()
                    model.chat.beforeQueuedAction = { queued ->
                        if (queued.matchesInput(action)) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.chat.onQueuedActionFinished =
                        { queued -> if (queued.matchesInput(action)) finished.complete(Unit) }
                    model.group.beforeGroupWrite = { queued, index ->
                        if (index == 0 && queued.matchesInput(action)) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.group.onGroupActionFinished =
                        { queued -> if (queued.matchesInput(action)) finished.complete(Unit) }
                    model.conversations.beforeConsentDecision = {
                        if (action is MessengerAction.Consent) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.conversations.onConsentFinished =
                        { if (action is MessengerAction.Consent) finished.complete(Unit) }
                    model.dispatch(action)
                    withTimeout(30_000) { entered.await() }
                    openChat(b.id())
                    until("current parent visible") {
                        model.state.value.messages
                            .any { it.id == bParent }
                    }
                    model.dispatch(MessengerAction.Reply(bParent))
                    release.complete(Unit)
                    withTimeout(30_000) { finished.await() }
                    assertEquals(
                        "A rows after $action",
                        beforeA,
                        Conversation
                            .Group(a)
                            .messages()
                            .map { it.id }
                            .toSet(),
                    )
                    assertEquals(
                        "B rows after $action",
                        beforeB,
                        Conversation
                            .Group(b)
                            .messages()
                            .map { it.id }
                            .toSet(),
                    )
                    val parent = checkNotNull(owner.client.conversations.getMessageById(target))
                    assertEquals("Origin parent", parent.toRow(owner.client.inboxId()).text)
                    assertFalse(parent.toRow(owner.client.inboxId()).deleted)
                    assertTrue(parent.reactions.isEmpty())
                    assertEquals("Origin A", a.state().name)
                    assertEquals("Current B", b.state().name)
                    assertEquals("Original", a.state().description)
                    assertEquals("Original", b.state().description)
                    assertEquals(ConsentState.ALLOWED, a.state().common.consentState)
                    assertEquals(ConsentState.ALLOWED, b.state().common.consentState)
                    assertNull(a.state().common.disappearingSettings)
                    assertNull(b.state().common.disappearingSettings)
                    assertEquals(GroupPolicyType.ALL_MEMBERS, a.state().permissions.policyType)
                    assertEquals(GroupPolicyType.ALL_MEMBERS, b.state().permissions.policyType)
                    assertNotEquals(
                        DeliveryStatus.PUBLISHED,
                        owner.client.conversations
                            .getMessageById(pending)
                            ?.deliveryStatus,
                    )
                    assertTrue(
                        model.session.preferences
                            .drafts(owner.key.profileId)
                            .isEmpty(),
                    )
                    assertEquals(bParent, model.state.value.replyTo)
                    println("ACTION_SCOPE_PROOF stage=queued-rejected action=$action")
                }
                val createEntered = CompletableDeferred<Unit>()
                val createRelease = CompletableDeferred<Unit>().also(releases::add)
                val holdResolution =
                    java.util.concurrent.atomic
                        .AtomicBoolean(true)
                val nativeQueries = XmtpQueries()
                val createQueries =
                    object : ConversationQueries by nativeQueries {
                        override suspend fun resolveInbox(
                            current: ActiveSession,
                            address: String,
                        ): String? {
                            val resolved = nativeQueries.resolveInbox(current, address)
                            assertEquals(owner.client.inboxId(), resolved)
                            if (holdResolution.compareAndSet(true, false)) {
                                createEntered.complete(Unit)
                                createRelease.await()
                            }
                            return resolved
                        }
                    }
                val createController =
                    CreateConversationController(
                        model.session,
                        createQueries,
                        Semaphore(4),
                        graphRule.graph.destinations,
                    )
                model.dispatch(MessengerAction.Navigate(Screen.CREATE))
                until("create destination") { model.state.value.screen == Screen.CREATE }
                createController.open()
                createController.dispatch(CreateConversationAction.ChangeRecipients(owner.client.identity().identifier))
                createController.dispatch(CreateConversationAction.ResolveRecipients)
                withTimeout(30_000) { createEntered.await() }
                assertTrue(createController.state.value.busy)
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("leave held create lookup") { model.state.value.screen == Screen.CONVERSATIONS }
                createRelease.complete(Unit)
                until("create lookup finished outside destination") { !createController.state.value.busy }
                assertTrue(
                    createController.state.value.selectedRecipients
                        .isEmpty(),
                )
                model.dispatch(MessengerAction.Navigate(Screen.CREATE))
                until("reopen create destination") { model.state.value.screen == Screen.CREATE }
                createController.open()
                assertFalse(createController.state.value.busy)
                createController.dispatch(CreateConversationAction.ResolveRecipients)
                until("reopened create resolves actual inbox") {
                    !createController.state.value.busy &&
                        createController.state.value.selectedRecipients
                            .any { it.inboxId == owner.client.inboxId() }
                }
                assertEquals(
                    owner.client.inboxId(),
                    createController.state.value.selectedRecipients
                        .single()
                        .inboxId,
                )

                model.dispatch(MessengerAction.Navigate(Screen.APP_SETTINGS))
                until("settings destination") { model.state.value.screen == Screen.APP_SETTINGS }
                val unknown = SendDraftRef(UUID.randomUUID().toString(), a.id(), phase = SendPhase.QUEUEING)
                model.session.preferences.saveDraft(owner.key.profileId, unknown) { change ->
                    model.session.admit(owner.key, change)
                }
                model.settings.refresh(owner)
                assertTrue(
                    model.settings.state.value.unknownSends
                        .any { it.draftId == unknown.draftId },
                )
                val settingsEntered = CompletableDeferred<Unit>()
                val settingsRelease = CompletableDeferred<Unit>().also(releases::add)
                model.session.preferences.beforeDraftCommit = {
                    settingsEntered.complete(Unit)
                    settingsRelease.await()
                }
                model.dispatch(MessengerAction.DiscardUnknownSend(unknown.draftId))
                withTimeout(30_000) { settingsEntered.await() }
                assertTrue(model.settings.state.value.busy)
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATIONS))
                until("leave held settings command") { model.state.value.screen == Screen.CONVERSATIONS }
                settingsRelease.complete(Unit)
                until("settings command finished outside destination") { !model.settings.state.value.busy }
                model.session.preferences.beforeDraftCommit = {}
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .none { it.draftId == unknown.draftId },
                )
                model.dispatch(MessengerAction.Navigate(Screen.APP_SETTINGS))
                until("reopened settings is ready") {
                    model.state.value.screen == Screen.APP_SETTINGS && !model.settings.state.value.busy &&
                        model.settings.state.value.unknownSends
                            .none { it.draftId == unknown.draftId }
                }
                assertEquals(
                    beforeA,
                    Conversation
                        .Group(a)
                        .messages()
                        .map { it.id }
                        .toSet(),
                )
                assertEquals(
                    beforeB,
                    Conversation
                        .Group(b)
                        .messages()
                        .map { it.id }
                        .toSet(),
                )
                println("ACTION_SCOPE_PROOF stage=create-and-settings-completion-keeps-reopened-owner-ready")
            } finally {
                releases.forEach { it.complete(Unit) }
                model.session.preferences.beforeDraftCommit = {}
                cleanup()
            }
        }

    @Test fun suspendedTargetsCannotMutateAfterSwitch() =
        runBlocking {
            val releases = mutableListOf<CompletableDeferred<Unit>>()
            try {
                val owner = connect()
                val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Read A"))
                val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Read B"))
                val target = a.sendText("Read parent")
                val pending = a.sendText("Read pending", SendOptions(optimistic = true))
                val beforeA =
                    Conversation
                        .Group(a)
                        .messages()
                        .map { it.id }
                        .toSet()
                val beforeB =
                    Conversation
                        .Group(b)
                        .messages()
                        .map { it.id }
                        .toSet()
                val actions =
                    listOf(
                        MessengerAction.React(target, "❤️", false),
                        MessengerAction.DeleteMessage(target),
                        MessengerAction.SendText("Late reply"),
                        MessengerAction.RetrySend(pending),
                    )
                for (action in actions) {
                    openChat(a.id())
                    until("bound action target") {
                        assertNull("Chat open must succeed before action dispatch", model.chat.state.value.failure)
                        currentChatTargetReady(model, graphRule.graph, owner, a.id()) &&
                            model.state.value.chatState
                                ?.let { it.composer.enabled && !it.unknown } == true
                    }
                    until("origin parent visible") {
                        model.state.value.messages
                            .any { it.id == target }
                    }
                    model.dispatch(MessengerAction.Reply(target))
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    val finished = CompletableDeferred<Unit>()

                    suspend fun read(
                        client: SDKClient,
                        id: MessageId,
                    ): Message? {
                        val message = client.conversations.getMessageById(id)
                        entered.complete(Unit)
                        release.await()
                        return message
                    }
                    model.chat.actionMessageRead = { current, id -> read(current.client, id) }
                    model.sends.messageRead = ::read
                    model.chat.onQueuedActionFinished =
                        { queued -> if (queued.matchesInput(action)) finished.complete(Unit) }
                    model.dispatch(action)
                    withTimeout(30_000) { entered.await() }
                    openChat(b.id())
                    release.complete(Unit)
                    withTimeout(30_000) { finished.await() }
                    assertEquals(
                        "A rows after suspended $action",
                        beforeA,
                        Conversation
                            .Group(a)
                            .messages()
                            .map { it.id }
                            .toSet(),
                    )
                    assertEquals(
                        "B rows after suspended $action",
                        beforeB,
                        Conversation
                            .Group(b)
                            .messages()
                            .map { it.id }
                            .toSet(),
                    )
                    val parent = checkNotNull(owner.client.conversations.getMessageById(target))
                    assertFalse(parent.toRow(owner.client.inboxId()).deleted)
                    assertTrue(parent.reactions.isEmpty())
                    assertNotEquals(
                        DeliveryStatus.PUBLISHED,
                        owner.client.conversations
                            .getMessageById(pending)
                            ?.deliveryStatus,
                    )
                    assertTrue(
                        model.session.preferences
                            .drafts(owner.key.profileId)
                            .isEmpty(),
                    )
                    println("ACTION_SCOPE_PROOF stage=read-completed-without-write action=$action")
                    model.chat.actionMessageRead = { current, id -> current.client.conversations.getMessageById(id) }
                    model.sends.messageRead = { client, id -> client.conversations.getMessageById(id) }
                }
            } finally {
                releases.forEach { it.complete(Unit) }
                cleanup()
            }
        }

    @Test fun currentActionsWriteNativeTargetsAndKeepCapturedReply() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val group =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Current actions"),
                    )
                val original = group.sendText("Original reply parent")
                val next = group.sendText("Next reply parent")
                val pending = group.sendText("Current retry", SendOptions(optimistic = true))
                openChat(group.id())
                until("reply parents visible") {
                    model.state.value.messages
                        .any { it.id == original } &&
                        model.state.value.messages
                            .any { it.id == next }
                }
                model.dispatch(MessengerAction.Reply(original))
                assertEquals(original, model.state.value.replyTo)
                val action = MessengerAction.SendText("Captured reply")
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Unit>()
                model.chat.beforeQueuedAction = { queued ->
                    if (queued.matchesInput(action)) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                model.chat.onQueuedActionFinished =
                    { queued -> if (queued.matchesInput(action)) finished.complete(Unit) }
                model.dispatch(action)
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Reply(next))
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                val nativeRows = Conversation.Group(group).messages()
                println(
                    "REPLY_INTENT_PROOF result=${model.state.value.textSendResult} error=${model.state.value.error}",
                )
                println("REPLY_INTENT_PROOF rows=${nativeRows.map { it.deliveryStatus to it.content }}")
                until("actual captured reply publication") {
                    Conversation.Group(group).messages(publishedSelection()).any {
                        (it.content as? SDKMessageContent.Standard)?.value is MessageContent.Reply
                    }
                }
                val replies =
                    Conversation.Group(group).messages(publishedSelection()).mapNotNull {
                        ((it.content as? SDKMessageContent.Standard)?.value as? MessageContent.Reply)
                    }
                assertEquals(
                    listOf(MessageContent.Reply(original, MessageBody.Text("Captured reply"))),
                    replies,
                )
                assertEquals(next, model.state.value.replyTo)
                model.chat.beforeQueuedAction = {}

                // Hide only the rendered parent. The SDK still owns its readable body.
                model.chat.historyPageRead = { chat, options, before, after ->
                    val page = chat.historyPage(options, before, after)
                    val visible = page.messages.filter { it.id != next }
                    page.copy(
                        messages = visible,
                        skippedCount = page.skippedCount + (page.messages.size - visible.size).toUInt(),
                    )
                }
                model.chat.refresh(owner)
                until("reply parent outside the rendered window") {
                    model.state.value.messages
                        .none { it.id == next }
                }
                assertEquals(
                    "Next reply parent",
                    checkNotNull(owner.client.conversations.getMessageById(next)).toRow(owner.client.inboxId()).text,
                )
                assertEquals(next, model.state.value.replyTo)
                assertEquals("Next reply parent", model.state.value.replyPreview)
                compose.activityRule.scenario.recreate()
                until("recreated reply keeps the native parent") {
                    model.state.value.replyTo == next && model.state.value.replyPreview == "Next reply parent"
                }
                compose.onNodeWithText("Next reply parent", substring = false).assertExists()
                compose.onNodeWithText("Message").performTextReplacement("Reply to retained native parent")
                compose.onNodeWithContentDescription("Send message").performClick()
                until("off-window reply publishes to its native parent") {
                    Conversation.Group(group).messages(publishedSelection()).any {
                        it.standardContent() ==
                            MessageContent.Reply(next, MessageBody.Text("Reply to retained native parent"))
                    }
                }
                val restoredReplies =
                    Conversation.Group(group).messages(publishedSelection()).filter {
                        it.standardContent() ==
                            MessageContent.Reply(next, MessageBody.Text("Reply to retained native parent"))
                    }
                assertEquals(1, restoredReplies.size)
                assertEquals(next, restoredReplies.single().inReplyTo?.id)
                model.chat.historyPageRead =
                    { chat, options, before, after -> chat.historyPage(options, before, after) }
                println("REPLY_INTENT_PROOF stage=off-window-parent-refresh-recreation-keeps-native-reply")

                suspend fun dispatchCurrent(current: MessengerAction) {
                    val done = CompletableDeferred<Unit>()
                    if (current.isGroupInput()) {
                        openDetails(group.id())
                        model.group.onGroupActionFinished =
                            { queued -> if (queued.matchesInput(current)) done.complete(Unit) }
                    } else if (current is MessengerAction.Consent) {
                        openChat(group.id())
                        model.conversations.onConsentFinished = { done.complete(Unit) }
                    } else {
                        openChat(group.id())
                        model.chat.onQueuedActionFinished =
                            { queued -> if (queued.matchesInput(current)) done.complete(Unit) }
                    }
                    model.dispatch(current)
                    withTimeout(30_000) { done.await() }
                    assertNull("Current action error: $current", model.state.value.error)
                }
                dispatchCurrent(MessengerAction.UpdateGroup("Updated current", "Updated description"))
                assertEquals("Updated current", group.state().name)
                assertEquals("Updated description", group.state().description)
                dispatchCurrent(MessengerAction.SetDisappearing(60))
                assertEquals(
                    60_000_000_000L,
                    group
                        .state()
                        .common.disappearingSettings
                        ?.retentionNs,
                )
                dispatchCurrent(MessengerAction.SetDisappearing(0))
                dispatchCurrent(MessengerAction.SetPreset(true))
                assertEquals(GroupPolicyType.ADMIN_ONLY, group.state().permissions.policyType)
                dispatchCurrent(MessengerAction.RetrySend(pending))
                assertEquals(
                    DeliveryStatus.PUBLISHED,
                    owner.client.conversations
                        .getMessageById(pending)
                        ?.deliveryStatus,
                )
                dispatchCurrent(MessengerAction.React(original, "👍", false))
                assertTrue(
                    checkNotNull(owner.client.conversations.getMessageById(original)).reactions.any {
                        it.reaction.content == "👍"
                    },
                )
                dispatchCurrent(MessengerAction.DeleteMessage(next))
                assertTrue(
                    checkNotNull(owner.client.conversations.getMessageById(next)).toRow(owner.client.inboxId()).deleted,
                )
                dispatchCurrent(MessengerAction.Consent(false))
                assertEquals(ConsentState.DENIED, group.state().common.consentState)
                println("ACTION_SCOPE_PROOF stage=current-sdk-writes-and-captured-reply-succeeded")
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun acceptedSendKeepsOriginalIdAfterScreenSwitch() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val a =
                    Conversation.Group(
                        owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Accepted A")),
                    )
                val b =
                    owner.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Accepted B"),
                    )
                openChat(a.id())
                val token = model.chat.screenToken()
                val accepted = CompletableDeferred<MessageId>()
                val task =
                    async {
                        runCatching {
                            model.sends.queue(
                                owner.key,
                                owner.client,
                                a,
                                admission = {
                                    model.chat.acceptsScreen(
                                        owner.key,
                                        token,
                                    )
                                },
                                reconcile = {},
                            ) {
                                val id = a.sendText("Accepted before switch", SendOptions(optimistic = true))
                                accepted.complete(id)
                                release.await()
                                id
                            }
                        }
                    }
                val id = withTimeout(30_000) { accepted.await() }
                openChat(b.id())
                release.complete(Unit)
                val outcome = withTimeout(30_000) { task.await() }
                assertTrue(outcome.exceptionOrNull() is CancellationException)
                val draft =
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .single()
                assertEquals(SendPhase.ACCEPTED, draft.phase)
                assertEquals(id, draft.acceptedMessageId)
                assertEquals(a.id(), draft.conversationKey)
                assertNotNull(owner.client.conversations.getMessageById(id))
                assertEquals(0uL, Conversation.Group(b).countMessages(publishedSelection()))
                model.sends.retry(owner.key, owner.client, a, id) {}
                assertEquals(
                    DeliveryStatus.PUBLISHED,
                    owner.client.conversations
                        .getMessageById(id)
                        ?.deliveryStatus,
                )
                assertTrue(
                    model.session.preferences
                        .drafts(owner.key.profileId)
                        .isEmpty(),
                )
                println("ACTION_SCOPE_PROOF stage=accepted-original-id-saved-and-retried id=$id")
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun lateBlockedConsentCannotReplaceNewChatSettings() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Blocked A"))
                val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Current B"))
                model.dispatch(MessengerAction.OpenConversation(a.id()))
                until("open A with bound consent target") {
                    model.chat.currentConversation()?.id() == a.id() &&
                        model.conversations.requestState.value.conversationId == a.id()
                }
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Unit>()
                model.conversations.writeConsent = { chat, value ->
                    entered.complete(Unit)
                    release.await()
                    chat.updateConsentState(value)
                }
                model.conversations.onConsentFinished = { finished.complete(Unit) }
                model.dispatch(MessengerAction.Consent(false))
                until("held A consent SDK write") { entered.isCompleted }
                model.dispatch(MessengerAction.OpenConversation(b.id()))
                until("open B with bound consent target") {
                    model.chat.currentConversation()?.id() == b.id() &&
                        model.conversations.requestState.value.conversationId == b.id()
                }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                until("settings B") { model.state.value.settings.title == "Current B" }
                release.complete(Unit)
                until("old A consent SDK write finished") { finished.isCompleted }
                println("CONSENT_PROOF stage=old-sdk-write-completed screen=${model.state.value.screen}")
                assertEquals(ConsentState.DENIED, a.state().common.consentState)
                assertEquals(b.id(), model.state.value.conversationId)
                assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)
                val beforeMissingConsent = b.state().common.consentState
                val missingId =
                    UUID
                        .randomUUID()
                        .toString()
                        .replace("-", "")
                assertEquals(b.id().length, missingId.length)
                assertTrue(missingId.all { it in '0'..'9' || it in 'a'..'f' })
                assertNull(owner.client.conversations.getById(missingId))
                model.graph.destinations.navigate(AppRoute.Chat(missingId))
                model.conversations.prepareRequest(owner, missingId, "Removed request")
                val missingFinished = CompletableDeferred<Unit>()
                model.conversations.onConsentFinished = { missingFinished.complete(Unit) }
                model.conversations.dispatch(RequestAction.Block)
                assertEquals(RequestDecision.BLOCK, model.conversations.requestState.value.pendingDecision)
                model.conversations.dispatch(RequestAction.Confirm)
                withTimeout(30_000) { missingFinished.await() }
                val missingState = model.conversations.requestState.value
                assertNull("A missing request must clear its confirmation", missingState.pendingDecision)
                assertEquals("This request is no longer available.", missingState.failure?.message)
                assertEquals("request-decision", missingState.failure?.code)
                assertEquals(beforeMissingConsent, b.state().common.consentState)
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    @Test fun failedResetOffersRealUiRetryWithoutAnOwnerAndKeepsPeerProfile() =
        runBlocking {
            var link: File? = null
            var peerRoot: File? = null
            var peerId: String? = null
            try {
                val owner = connect()
                val peer = BackendProfile(UUID.randomUUID().toString(), "http://peer.invalid")
                peerId = peer.id
                model.session.preferences.saveProfile(peer)
                peerRoot = peer.paths(compose.activity.filesDir).root.apply { mkdirs() }
                val sentinel = File(peerRoot, "keep").apply { writeText("peer data") }
                link = File(owner.paths.temp, "reset-blocker")
                owner.paths.temp.mkdirs()
                Files.createSymbolicLink(link.toPath(), sentinel.toPath())
                model.dispatch(MessengerAction.DeleteAccount)
                until("failed reset banner") {
                    model.state.value.error != null && model.state.value.pendingReset && !model.state.value.busy
                }
                assertNull(model.session.active.value)
                assertNotNull(model.session.preferences.reset())
                assertEquals("peer data", sentinel.readText())
                assertEquals("Local data removal is not complete. Try again.", model.state.value.error)
                assertEquals(model.state.value.error, checkNotNull(model.state.value.connectState).failure?.message)
                assertTrue(checkNotNull(model.state.value.connectState).pendingReset)
                assertEquals(Screen.START, model.state.value.screen)
                assertEquals(
                    ResetPhase.DATABASE_REMOVED,
                    model.session.preferences
                        .reset()
                        ?.phase,
                )
                Files.delete(link.toPath())
                link = null
                compose
                    .onNode(
                        (hasText("Continue deletion") or hasText("Continue local data removal")) and hasClickAction(),
                    ).performScrollTo()
                    .performClick()
                println("RESET_PROOF stage=retry-clicked pending=${model.state.value.pendingReset}")
                until("reset retry complete") { !model.state.value.pendingReset && model.state.value.error == null }
                assertNull(model.session.preferences.reset())
                assertFalse(owner.paths.root.exists())
                assertFalse(
                    model.session.preferences
                        .profiles()
                        .any { it.id == owner.profile.id },
                )
                assertTrue(
                    model.session.preferences
                        .profiles()
                        .any { it.id == peer.id },
                )
                assertEquals("peer data", sentinel.readText())

                // Exercise recovery in a fresh process graph with no SDK client.
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val cold = checkNotNull(model.session.active.value)
                model.session.signOut()
                cold.paths.temp.mkdirs()
                link = File(cold.paths.temp, "cold-reset-blocker")
                Files.createSymbolicLink(link.toPath(), sentinel.toPath())
                model.session.preferences.saveReset(cold.paths.resetRecord(cold.profile.id))
                val previousGraph = graphRule.graph
                compose.runOnUiThread { compose.activity.viewModelStore.clear() }
                val freshGraph = graphRule.replaceGraph()
                assertNotSame(previousGraph, freshGraph)
                compose.activityRule.scenario.recreate()
                assertSame(freshGraph.session, model.session)
                until("cold failed reset banner") { model.state.value.pendingReset && model.state.value.error != null }
                assertEquals("Cannot restore this account. Try again.", model.state.value.error)
                assertEquals(model.state.value.error, checkNotNull(model.state.value.connectState).failure?.message)
                assertTrue(checkNotNull(model.state.value.connectState).pendingReset)
                assertNull(model.session.active.value)
                assertEquals(
                    ResetPhase.DATABASE_REMOVED,
                    model.session.preferences
                        .reset()
                        ?.phase,
                )
                assertEquals("peer data", sentinel.readText())
                Files.delete(link.toPath())
                link = null
                compose
                    .onNode(
                        (hasText("Continue deletion") or hasText("Continue local data removal")) and hasClickAction(),
                    ).performScrollTo()
                    .performClick()
                until(
                    "cold reset retry complete",
                ) { !model.state.value.pendingReset && model.state.value.error == null }
                assertNull(model.session.preferences.reset())
                assertFalse(cold.paths.root.exists())
                assertTrue(
                    model.session.preferences
                        .profiles()
                        .any { it.id == peer.id },
                )
                assertEquals("peer data", sentinel.readText())
                println("RESET_PROOF stage=cold-ui-retry-completed owner=false peer-preserved=true")
            } finally {
                link?.let { Files.deleteIfExists(it.toPath()) }
                cleanup()
                peerId?.let { model.session.preferences.removeProfile(it) }
                peerRoot?.deleteRecursively()
            }
        }

    @Test fun blockedAnchorEditCannotCommitAfterNavigation() =
        runBlocking {
            val release = CompletableDeferred<Unit>()
            try {
                val owner = connect()
                val chat = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Anchor"))
                val id = chat.sendText("Anchor body")
                model.dispatch(MessengerAction.OpenConversation(chat.id()))
                until("anchor timeline") {
                    model.state.value.messages
                        .any { it.id == id }
                }
                val previous = model.session.preferences.anchor(owner.key.profileId, chat.id())
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Boolean>()
                model.session.preferences.beforePositionCommit = { key ->
                    if (key.contains("/scroll/")) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                model.session.preferences.positionCommitFinished = { key, accepted ->
                    if (key.contains("/scroll/")) finished.complete(accepted)
                }
                model.dispatch(MessengerAction.Viewport(ScrollAnchor(id, 42, 17, false), false))
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.Navigate(Screen.APP_SETTINGS))
                release.complete(Unit)
                val accepted = withTimeout(30_000) { finished.await() }
                val actual = model.session.preferences.anchor(owner.key.profileId, chat.id())
                println("POSITION_PROOF kind=anchor accepted=$accepted previous=$previous actual=$actual")
                assertFalse(accepted)
                assertEquals(previous, actual)
                assertEquals(Screen.APP_SETTINGS, model.state.value.screen)
            } finally {
                release.complete(Unit)
                cleanup()
            }
        }

    private suspend fun blockedRead(invalidate: Boolean) {
        var peer: SDKClient? = null
        var reader: Job? = null
        val release = CompletableDeferred<Unit>()
        val unregisterRelease = CompletableDeferred<Unit>()
        var stop: Deferred<Unit>? = null
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
            reader =
                CoroutineScope(
                    currentCoroutineContext(),
                ).launch { second.conversations.streamAllMessages().collect { } }
            val remote = second.conversations.createDm(owner.client.inboxId())
            val id = remote.sendText("Guarded incoming read")
            val stored =
                withTimeout(30_000) {
                    var value: Message? = null
                    while (value == null) {
                        value = owner.client.conversations.getMessageById(id)
                        delay(20)
                    }
                    value
                }
            val chat = checkNotNull(owner.client.conversations.getById(stored.conversationId))
            chat.updateConsentState(ConsentState.ALLOWED)
            val key = logicalConversationKey(chat, owner.client.inboxId())
            model.dispatch(MessengerAction.OpenConversation(chat.id()))
            until("read timeline") {
                model.state.value.messages
                    .any { it.id == id }
            }
            val previous = model.session.preferences.marker(owner.key.profileId, key)
            val entered = CompletableDeferred<Unit>()
            val finished = CompletableDeferred<Boolean>()
            model.session.preferences.beforePositionCommit = { value ->
                if (value.contains("/read/")) {
                    entered.complete(Unit)
                    release.await()
                }
            }
            model.session.preferences.positionCommitFinished = { value, accepted ->
                if (value.contains("/read/")) finished.complete(accepted)
            }
            model.foreground(true)
            model.dispatch(MessengerAction.Viewport(ScrollAnchor(id, stored.sentAt.ns, 0, true), true))
            withTimeout(30_000) { entered.await() }
            if (invalidate) {
                val unregisterEntered = CompletableDeferred<Unit>()
                val invalidated = CompletableDeferred<Unit>()
                graphRule.observers.onSessionInvalidated = { invalidated.complete(Unit) }
                graphRule.observers.beforeUnregister = {
                    unregisterEntered.complete(Unit)
                    unregisterRelease.await()
                }
                stop = CoroutineScope(currentCoroutineContext()).async { model.session.signOut() }
                withTimeout(30_000) { invalidated.await() }
                assertFalse(model.session.accepts(owner.key))
                println("POSITION_PROOF stage=generation-invalidated-before-marker-release")
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                withTimeout(30_000) { unregisterEntered.await() }
            } else {
                model.foreground(false)
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
            }
            val accepted = finished.await()
            val actual = model.session.preferences.marker(owner.key.profileId, key)
            println(
                "POSITION_PROOF kind=read invalidated=$invalidate accepted=$accepted previous=$previous actual=$actual",
            )
            assertFalse(accepted)
            assertEquals(previous, actual)
        } finally {
            release.complete(Unit)
            unregisterRelease.complete(Unit)
            stop?.await()
            reader?.cancelAndJoin()
            withContext(NonCancellable) { peer?.end() }
            cleanup()
        }
    }

    @Test fun blockedReadEditCannotCommitAfterBackgrounding() = runBlocking { blockedRead(false) }

    @Test fun blockedReadEditCannotCommitAfterSessionInvalidation() = runBlocking { blockedRead(true) }
}
