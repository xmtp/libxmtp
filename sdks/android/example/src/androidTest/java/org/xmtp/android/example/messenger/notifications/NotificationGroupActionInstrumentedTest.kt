package org.xmtp.android.example.messenger.notifications

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.messenger.*
import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.Screen
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.Base64

class NotificationGroupActionInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    private suspend fun until(
        stage: String,
        check: () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $stage", error)
        }
    }

    private suspend fun pausedAction(membership: Boolean) {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        val app = compose.activity.application as ExampleApp
        val session = app.session
        val previousController = app.notifications
        val previousMessage = session.onMessage
        val previousEvent = session.onEvent
        val previousInvalidated = session.onInvalidated
        val previousAdmission = session.onNotificationAdmissionChanged
        val previousEnd = session.beforeEnd
        val previousUnregister = session.unregisterNotifications
        val previousPreflight = session.needsNotificationPreflight
        val previousStop = session.stopStoredNotifications
        val previousRemoval = session.beforeProfileRemoval
        val store = ViewModelStore()
        val releases = mutableListOf<CompletableDeferred<Unit>>()
        var controller: NotificationController? = null
        var model: MessengerViewModel? = null
        var peer: SDKClient? = null

        fun install(value: NotificationController) {
            ExampleApp::class.java
                .getDeclaredField("notifications")
                .apply { isAccessible = true }
                .set(app, value)
        }
        try {
            session.signOut()
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(session.active.value)
            val transport =
                object : PushTransport {
                    override val configured = true

                    override fun requestToken(callback: (String?, Throwable?) -> Unit) = error("No FCM token request")
                }
            val notifications = NotificationController(app, session, transport)
            controller = notifications
            notifications.permissionGranted = { true }
            notifications.enableRegistration = { _, _ -> NotificationState.Enabled }
            notifications.disableRegistration = { }
            notifications.readConversationState = { chat -> conversationState(chat).copy(notificationsEnabled = true) }
            notifications.tokenChanged("compile-only-token")
            install(notifications)
            val fresh =
                ViewModelProvider(
                    store,
                    ViewModelProvider.AndroidViewModelFactory.getInstance(app),
                )[MessengerViewModel::class.java]
            model = fresh
            until("current SDK owner projection") { fresh.state.value.inbox == owner.client.inboxId() }
            fresh.foreground(false)
            owner.work.async { notifications.setEnabled(owner, true) }.await()
            val group =
                owner.client.conversations.createGroup(
                    emptyList(),
                    CreateGroupOptions(name = "Before edit", description = "Before description"),
                )
            val groupId = group.id()
            fresh.dispatch(MessengerAction.OpenConversation(groupId))
            until("actual group timeline") {
                fresh.state.value.conversationId == groupId &&
                    fresh.state.value.screen == Screen.TIMELINE
            }
            var posts = 0
            notifications.postNotification = { _, _, _ ->
                posts += 1
                true
            }
            var sequence = 0

            suspend fun hold(
                action: MessengerAction,
                index: Int,
                verifyPaused: suspend () -> Unit,
            ) {
                val entered = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>().also(releases::add)
                val finished = CompletableDeferred<Unit>()
                fresh.beforeGroupWrite = { actual, write ->
                    if (actual == action && write == index) {
                        entered.complete(Unit)
                        release.await()
                    }
                }
                fresh.onQueuedActionFinished = { actual -> if (actual == action) finished.complete(Unit) }
                fresh.dispatch(action)
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
                hold(MessengerAction.AddMember(peerId), 0) { assertFalse(group.members().any { it.inboxId == peerId }) }
                assertTrue(group.members().any { it.inboxId == peerId })
                assertEquals(0, posts)
            } else {
                hold(MessengerAction.UpdateGroup("First name", "First description"), 0) {
                    assertEquals("Before edit", group.state().name)
                }
                assertEquals("First name", group.state().name)
                assertEquals("First description", group.state().description)
                hold(MessengerAction.UpdateGroup("Second name", "Second description"), 1) {
                    assertEquals("Second name", group.state().name)
                    assertEquals("First description", group.state().description)
                }
                assertEquals("Second description", group.state().description)
                hold(MessengerAction.SetDisappearing(60), 0) { assertNull(group.state().common.disappearingSettings) }
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
            withContext(NonCancellable) {
                model?.beforeGroupWrite = { _, _ -> }
                model?.onQueuedActionFinished = {}
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
                    controller?.close()
                    install(previousController)
                    session.onMessage = previousMessage
                    session.onEvent = previousEvent
                    session.onInvalidated = previousInvalidated
                    session.onNotificationAdmissionChanged = previousAdmission
                    session.beforeEnd = previousEnd
                    session.unregisterNotifications = previousUnregister
                    session.needsNotificationPreflight = previousPreflight
                    session.stopStoredNotifications = previousStop
                    session.beforeProfileRemoval = previousRemoval
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
    }

    @Test fun pausedMetadataEditsKeepEligibleNativePushes() = runBlocking { pausedAction(false) }

    @Test fun pausedNativeMemberAddStillBlocksPushUntilCommit() = runBlocking { pausedAction(true) }
}
