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

class NotificationActionInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    private suspend fun until(
        name: String,
        check: () -> Boolean,
    ) {
        try {
            withTimeout(30_000) { while (!check()) delay(20) }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError("Stage: $name", error)
        }
    }

    private suspend fun rejectedAfterSwitch(stage: String) {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        val app = compose.activity.application as ExampleApp
        val session = app.session
        val originalNotifications = app.notifications
        val originalMessage = session.onMessage
        val originalEvent = session.onEvent
        val originalInvalidated = session.onInvalidated
        val originalAdmission = session.onNotificationAdmissionChanged
        val originalEnd = session.beforeEnd
        val originalUnregister = session.unregisterNotifications
        val originalPreflight = session.needsNotificationPreflight
        val originalStop = session.stopStoredNotifications
        val originalRemoval = session.beforeProfileRemoval
        val store = ViewModelStore()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        var controller: NotificationController? = null
        var model: MessengerViewModel? = null

        fun setNotifications(value: NotificationController) {
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

                    override fun requestToken(callback: (String?, Throwable?) -> Unit) =
                        error("This fixture must not request FCM")
                }
            val notifications = NotificationController(app, session, transport)
            controller = notifications
            setNotifications(notifications)
            val fresh =
                ViewModelProvider(
                    store,
                    ViewModelProvider.AndroidViewModelFactory.getInstance(app),
                )[MessengerViewModel::class.java]
            model = fresh
            until("fresh owner projection") { fresh.state.value.inbox == owner.client.inboxId() }
            fresh.foreground(false)
            val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Notification A"))
            val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Notification B"))

            suspend fun open(id: String) {
                fresh.dispatch(MessengerAction.OpenConversation(id))
                until("open $id") {
                    fresh.state.value.conversationId == id &&
                        fresh.state.value.screen == Screen.TIMELINE
                }
            }
            open(a.id())
            var writes = 0
            notifications.beforeConversationNotificationWrite = { writes += 1 }
            if (stage == "action") {
                fresh.beforeConversationNotificationAction = {
                    entered.complete(Unit)
                    release.await()
                }
            } else {
                notifications.beforeConversationPreferenceLookup = { _, _ ->
                    entered.complete(Unit)
                    release.await()
                }
            }
            val action = MessengerAction.Feature("conversation-notifications", "false")
            val finished = CompletableDeferred<Unit>()
            fresh.onQueuedActionFinished = { queued -> if (queued == action) finished.complete(Unit) }
            fresh.dispatch(action)
            withTimeout(30_000) { entered.await() }
            open(b.id())
            release.complete(Unit)
            withTimeout(30_000) { finished.await() }
            val prefs = NotificationPreferences(app)
            assertFalse("The stale $stage action must not mute A", prefs.muted(owner.key.profileId, a.id()))
            assertFalse("The stale $stage action must not redirect to B", prefs.muted(owner.key.profileId, b.id()))
            assertEquals("No SDK notification write for a stale $stage action", 0, writes)
            assertEquals(b.id(), fresh.state.value.conversationId)
            fresh.beforeConversationNotificationAction = {}
            notifications.beforeConversationPreferenceLookup = { _, _ -> }
            val accepted = CompletableDeferred<Unit>()
            fresh.onQueuedActionFinished = { queued -> if (queued == action) accepted.complete(Unit) }
            fresh.dispatch(action)
            withTimeout(30_000) { accepted.await() }
            assertTrue("The current B action must persist its mute", prefs.muted(owner.key.profileId, b.id()))
            assertFalse(prefs.muted(owner.key.profileId, a.id()))
            assertEquals("Current B must use the real SDK write", 1, writes)
            println("NOTIFICATION_ACTION stage=$stage stale_writes=0 current_B_writes=1")
        } finally {
            release.complete(Unit)
            withContext(NonCancellable) {
                model?.beforeConversationNotificationAction = {}
                model?.onQueuedActionFinished = {}
                controller?.beforeConversationPreferenceLookup = { _, _ -> }
                controller?.beforeConversationNotificationWrite = {}
                try {
                    if (session.preferences.active() != null ||
                        session.preferences.reset() != null
                    ) {
                        session.deleteAccount()
                    }
                    session.signOut()
                } finally {
                    store.clear()
                    controller?.close()
                    setNotifications(originalNotifications)
                    session.onMessage = originalMessage
                    session.onEvent = originalEvent
                    session.onInvalidated = originalInvalidated
                    session.onNotificationAdmissionChanged = originalAdmission
                    session.beforeEnd = originalEnd
                    session.unregisterNotifications = originalUnregister
                    session.needsNotificationPreflight = originalPreflight
                    session.stopStoredNotifications = originalStop
                    session.beforeProfileRemoval = originalRemoval
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
    }

    @Test fun capturedNotificationActionCannotRedirectAfterNavigation() = runBlocking { rejectedAfterSwitch("action") }

    @Test fun suspendedNotificationLookupCannotWriteAfterNavigation() = runBlocking { rejectedAfterSwitch("lookup") }
}
