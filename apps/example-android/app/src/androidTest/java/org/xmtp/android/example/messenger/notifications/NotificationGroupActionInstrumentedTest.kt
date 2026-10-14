package org.xmtp.android.example.messenger.notifications

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelStore
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.Screen
import org.xmtp.android.example.shared.ShellAction
import org.xmtp.android.example.shared.feature.group.GroupAction
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap

class NotificationGroupActionInstrumentedTest {
    private val transport =
        object : PushTransport {
            override val configured = true

            override fun requestToken(callback: (String?, Throwable?) -> Unit) = callback("compile-only-token", null)
        }
    private val graphRule = TestAppGraphRule(notificationTransport = transport)
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

    private suspend fun until(
        stage: String,
        timeoutMs: Long = 30_000,
        check: () -> Boolean,
    ) {
        try {
            withTimeout(timeoutMs) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun pausedAction(membership: Boolean) {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        val app = compose.activity.application as ExampleApp
        val session = app.session
        val store = ViewModelStore()
        val releases = mutableListOf<CompletableDeferred<Unit>>()
        val beforeEvent = graphRule.observers.beforeEvent
        var model: MessengerViewModel? = null
        var peer: SDKClient? = null

        try {
            session.signOut()
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(session.active.value)
            val notifications = graphRule.graph.notifications
            notifications.permissionGranted = { true }
            notifications.enableRegistration = { _, _ -> NotificationState.Enabled }
            notifications.disableRegistration = { }
            notifications.readConversationState = { chat -> conversationState(chat).copy(notificationsEnabled = true) }
            notifications.tokenChanged("compile-only-token")
            val fresh = graphRule.newShell(store)
            model = fresh
            until("current SDK owner projection") { fresh.state.value.inbox == owner.client.inboxId() }
            fresh.foreground(false)
            owner.work.async { notifications.setEnabled(owner, true) }.await()
            // The native consent event can change privacyRevision after createGroup returns.
            val allowedConversations = ConcurrentHashMap.newKeySet<String>()
            graphRule.observers.beforeEvent = { current, event ->
                if (current.key == owner.key && event is ClientEvent.ConsentChanged &&
                    event.consentChanged.entityKind == ConsentEntityKind.CONVERSATION &&
                    event.consentChanged.state == EventConsentState.ALLOWED
                ) {
                    allowedConversations.add(event.consentChanged.entity)
                }
            }
            val group =
                owner.client.conversations.createGroup(
                    emptyList(),
                    CreateGroupOptions(name = "Before edit", description = "Before description"),
                )
            val groupId = group.id()
            until("group creation consent admitted", timeoutMs = 10_000) { groupId in allowedConversations }
            fresh.dispatch(MessengerAction.ShellFeature(ShellAction.Navigate(AppRoute.ConversationDetails(groupId))))
            until("actual group details") {
                fresh.group.state.value.conversationId == groupId &&
                    fresh.state.value.screen == Screen.CONVERSATION_SETTINGS
            }
            var posts = 0
            notifications.postNotification = { _, _, _ ->
                posts += 1
                true
            }
            var sequence = 0

            suspend fun hold(
                action: GroupAction,
                index: Int,
                verifyPaused: suspend () -> Unit,
            ) {
                until("notification setup idle", timeoutMs = 10_000) { notifications.isIdle }
                val entered = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>().also(releases::add)
                val finished = CompletableDeferred<Unit>()
                fresh.group.beforeGroupWrite = { actual, write ->
                    if (actual == action && write == index) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                fresh.group.onGroupActionFinished = { actual -> if (actual == action) finished.complete(Unit) }
                fresh.group.dispatch(action)
                withTimeout(30_000) { entered.await() }
                verifyPaused()
                sequence += 1
                val topic =
                    Base64.getEncoder().encodeToString(
                        byteArrayOf(0) + groupId.chunked(2).map { it.toInt(16).toByte() }.toByteArray(),
                    )
                val accepted = notifications.receive(mapOf("topic" to topic, "sequence_id" to sequence.toString()))
                assertEquals("Only a pending membership change must block this push", !membership, accepted)
                release.complete(Unit)
                withTimeout(30_000) { finished.await() }
                assertNull(fresh.state.value.error)
            }
            if (membership) {
                peer =
                    SDKClient.create(
                        app,
                        localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                        ClientOptions(
                            backend = BackendSource.Options(BackendOptions(BuildConfig.XMTP_BACKEND_URL)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val peerId = checkNotNull(peer).inboxId()
                hold(GroupAction.AddMember(peerId), 0) { assertFalse(group.members().any { it.inboxId == peerId }) }
                assertTrue(group.members().any { it.inboxId == peerId })
                assertEquals(0, posts)
            } else {
                hold(GroupAction.UpdateGroup("First name", "First description"), 0) {
                    assertEquals("Before edit", group.state().name)
                }
                assertEquals("First name", group.state().name)
                assertEquals("First description", group.state().description)
                hold(GroupAction.UpdateGroup("Second name", "Second description"), 1) {
                    assertEquals("Second name", group.state().name)
                    assertEquals("First description", group.state().description)
                }
                assertEquals("Second description", group.state().description)
                hold(GroupAction.SetDisappearing(60), 0) { assertNull(group.state().common.disappearingSettings) }
                assertEquals(
                    60_000_000_000L,
                    group
                        .state()
                        .common.disappearingSettings
                        ?.retentionNs,
                )
                assertEquals(3, posts)
            }
            println("GROUP_PUSH_ADMISSION membership=$membership native_writes_complete=true captured_posts=$posts")
        } finally {
            releases.forEach { it.complete(Unit) }
            graphRule.observers.beforeEvent = beforeEvent
            withContext(NonCancellable) {
                model?.group?.beforeGroupWrite = { _, _ -> }
                model?.group?.onGroupActionFinished = {}
                try {
                    if (session.preferences.active() != null ||
                        session.preferences.reset() != null
                    ) {
                        session.deleteAccount()
                    }
                    session.signOut()
                } finally {
                    peer?.end()
                    store.clear()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
    }

    @Test fun pausedMetadataEditsKeepEligibleNativePushes() = runBlocking { pausedAction(false) }

    @Test fun pausedNativeMemberAddStillBlocksPushUntilCommit() = runBlocking { pausedAction(true) }
}
