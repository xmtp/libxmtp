package org.xmtp.android.example.messenger

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*
import java.io.File
import java.nio.file.Files
import java.security.SecureRandom
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MessengerReviewInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
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
            model.writeConsent = { chat, value -> chat.updateConsentState(value) }
            model.onConsentFinished = {}
            model.beforeQueuedAction = {}
            model.beforeFeaturesUiUpdate = {}
            model.beforeGroupWrite = { _, _ -> }
            model.onQueuedActionFinished = {}
            model.actionMessageRead = { owner, id -> owner.client.conversations.getMessageById(id) }
            model.sends.messageRead = { client, id -> client.conversations.getMessageById(id) }
            model.session.onSessionInvalidated = {}
            model.session.unregisterNotifications = {}
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
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    val finished = CompletableDeferred<Unit>()
                    model.beforeGroupWrite = { current, index ->
                        if (current == action && index == 1) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.onQueuedActionFinished = { current -> if (current == action) finished.complete(Unit) }
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
                    until("origin parent visible") {
                        model.state.value.messages
                            .any { it.id == target }
                    }
                    model.dispatch(MessengerAction.Reply(target))
                    val entered = CompletableDeferred<Unit>()
                    val release = CompletableDeferred<Unit>().also(releases::add)
                    val finished = CompletableDeferred<Unit>()
                    model.beforeQueuedAction = { queued ->
                        if (queued == action) {
                            entered.complete(Unit)
                            release.await()
                        }
                    }
                    model.onQueuedActionFinished = { queued -> if (queued == action) finished.complete(Unit) }
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
            } finally {
                releases.forEach { it.complete(Unit) }
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
                    model.actionMessageRead = { current, id -> read(current.client, id) }
                    model.sends.messageRead = ::read
                    model.onQueuedActionFinished = { queued -> if (queued == action) finished.complete(Unit) }
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
                    model.actionMessageRead = { current, id -> current.client.conversations.getMessageById(id) }
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
                val featureEntered = CompletableDeferred<Unit>()
                val featureRelease = CountDownLatch(1)
                model.beforeFeaturesUiUpdate = {
                    featureEntered.complete(Unit)
                    check(featureRelease.await(30, TimeUnit.SECONDS))
                }
                val featureUpdate =
                    async(Dispatchers.IO) {
                        model.setFeatures(model.state.value.features)
                    }
                try {
                    withTimeout(30_000) { featureEntered.await() }
                    model.dispatch(MessengerAction.Reply(original))
                    assertEquals(original, model.state.value.replyTo)
                    featureRelease.countDown()
                    withTimeout(30_000) { featureUpdate.await() }
                    assertEquals(
                        "Reply survives a concurrent SDK host state update",
                        original,
                        model.state.value.replyTo,
                    )
                } finally {
                    featureRelease.countDown()
                    model.beforeFeaturesUiUpdate = {}
                }
                model.dispatch(MessengerAction.Reply(original))
                assertEquals(original, model.state.value.replyTo)
                val action = MessengerAction.SendText("Captured reply")
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Unit>()
                model.beforeQueuedAction = { queued ->
                    if (queued == action) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                model.onQueuedActionFinished = { queued -> if (queued == action) finished.complete(Unit) }
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
                model.beforeQueuedAction = {}

                suspend fun dispatchCurrent(current: MessengerAction) {
                    val done = CompletableDeferred<Unit>()
                    model.onQueuedActionFinished = { queued -> if (queued == current) done.complete(Unit) }
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
                val token = model.screenToken()
                val accepted = CompletableDeferred<MessageId>()
                val task =
                    async {
                        runCatching {
                            model.sends.queue(
                                owner.key,
                                owner.client,
                                a,
                                admission = {
                                    model.acceptsScreen(
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
                until("open A") { model.state.value.conversationId == a.id() }
                val entered = CompletableDeferred<Unit>()
                val finished = CompletableDeferred<Unit>()
                model.writeConsent = { chat, value ->
                    entered.complete(Unit)
                    release.await()
                    chat.updateConsentState(value)
                }
                model.onConsentFinished = { finished.complete(Unit) }
                model.dispatch(MessengerAction.Consent(false))
                withTimeout(30_000) { entered.await() }
                model.dispatch(MessengerAction.OpenConversation(b.id()))
                until("open B") { model.state.value.conversationId == b.id() }
                model.dispatch(MessengerAction.Navigate(Screen.CONVERSATION_SETTINGS))
                until("settings B") { model.state.value.settings.title == "Current B" }
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                println("CONSENT_PROOF stage=old-sdk-write-completed screen=${model.state.value.screen}")
                assertEquals(ConsentState.DENIED, a.state().common.consentState)
                assertEquals(b.id(), model.state.value.conversationId)
                assertEquals(Screen.CONVERSATION_SETTINGS, model.state.value.screen)
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
                until("failed reset banner") { model.state.value.error != null && model.state.value.pendingReset }
                assertNull(model.session.active.value)
                assertNotNull(model.session.preferences.reset())
                assertEquals("peer data", sentinel.readText())
                assertTrue(
                    model.state.value.error!!
                        .contains("symbolic link"),
                )
                Files.delete(link.toPath())
                link = null
                compose.onNodeWithText("Retry").performClick()
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

                // Exercise recovery in a new UI owner with no SDK client.
                model.session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val cold = checkNotNull(model.session.active.value)
                model.session.signOut()
                cold.paths.temp.mkdirs()
                link = File(cold.paths.temp, "cold-reset-blocker")
                Files.createSymbolicLink(link.toPath(), sentinel.toPath())
                model.session.preferences.saveReset(cold.paths.resetRecord(cold.profile.id))
                compose.runOnUiThread { compose.activity.viewModelStore.clear() }
                compose.activityRule.scenario.recreate()
                until("cold failed reset banner") { model.state.value.pendingReset && model.state.value.error != null }
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
                compose.onNodeWithText("Retry").performClick()
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
                model.session.onSessionInvalidated = { invalidated.complete(Unit) }
                model.session.unregisterNotifications = {
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
