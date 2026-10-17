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

class NotificationActionInstrumentedTest {
    private val transport =
        object : PushTransport {
            override val configured = true

            override fun requestToken(callback: (String?, Throwable?) -> Unit) =
                error("This fixture must not request FCM")
        }
    private val graphRule = TestAppGraphRule(notificationTransport = transport)
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule val rules: TestRule = RuleChain.outerRule(graphRule).around(compose)

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
        val store = ViewModelStore()
        val entered = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        var model: MessengerViewModel? = null

        try {
            session.signOut()
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(session.active.value)
            val notifications = graphRule.graph.notifications
            val fresh = graphRule.newShell(store)
            model = fresh
            until("fresh owner projection") { fresh.state.value.inbox == owner.client.inboxId() }
            fresh.foreground(false)
            val a = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Notification A"))
            val b = owner.client.conversations.createGroup(emptyList(), CreateGroupOptions(name = "Notification B"))

            suspend fun open(id: String) {
                fresh.dispatch(MessengerAction.ShellFeature(ShellAction.Navigate(AppRoute.ConversationDetails(id))))
                until("open details $id") {
                    fresh.group.state.value.conversationId == id &&
                        fresh.state.value.screen == Screen.CONVERSATION_SETTINGS
                }
            }
            open(a.id())
            var writes = 0
            notifications.beforeConversationNotificationWrite = { writes += 1 }
            if (stage == "action") {
                fresh.group.beforeConversationNotificationAction = {
                    entered.complete(Unit)
                    release.await()
                }
            } else {
                notifications.beforeConversationPreferenceLookup = { _, _ ->
                    entered.complete(Unit)
                    release.await()
                }
            }
            val action = GroupAction.SetNotifications(false)
            val finished = CompletableDeferred<Unit>()
            fresh.group.onGroupActionFinished = { queued -> if (queued == action) finished.complete(Unit) }
            fresh.group.dispatch(action)
            withTimeout(30_000) { entered.await() }
            open(b.id())
            release.complete(Unit)
            withTimeout(30_000) { finished.await() }
            val prefs = NotificationPreferences(app)
            assertFalse("The stale $stage action must not mute A", prefs.muted(owner.key.profileId, a.id()))
            assertFalse("The stale $stage action must not redirect to B", prefs.muted(owner.key.profileId, b.id()))
            assertEquals("No SDK notification write for a stale $stage action", 0, writes)
            assertEquals(b.id(), fresh.group.state.value.conversationId)
            fresh.group.beforeConversationNotificationAction = {}
            notifications.beforeConversationPreferenceLookup = { _, _ -> }
            val accepted = CompletableDeferred<Unit>()
            fresh.group.onGroupActionFinished = { queued -> if (queued == action) accepted.complete(Unit) }
            fresh.group.dispatch(action)
            withTimeout(30_000) { accepted.await() }
            assertTrue("The current B action must persist its mute", prefs.muted(owner.key.profileId, b.id()))
            assertFalse(prefs.muted(owner.key.profileId, a.id()))
            assertEquals("Current B must use the real SDK write", 1, writes)
            println("NOTIFICATION_ACTION stage=$stage stale_writes=0 current_B_writes=1")
        } finally {
            release.complete(Unit)
            withContext(NonCancellable) {
                model?.group?.beforeConversationNotificationAction = {}
                model?.group?.onGroupActionFinished = {}
                graphRule.graph.notifications.beforeConversationPreferenceLookup = { _, _ -> }
                graphRule.graph.notifications.beforeConversationNotificationWrite = {}
                try {
                    if (session.preferences.active() != null ||
                        session.preferences.reset() != null
                    ) {
                        session.deleteAccount()
                    }
                    session.signOut()
                } finally {
                    store.clear()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
    }

    @Test fun capturedNotificationActionCannotRedirectAfterNavigation() = runBlocking { rejectedAfterSwitch("action") }

    @Test fun suspendedNotificationLookupCannotWriteAfterNavigation() = runBlocking { rejectedAfterSwitch("lookup") }
}
