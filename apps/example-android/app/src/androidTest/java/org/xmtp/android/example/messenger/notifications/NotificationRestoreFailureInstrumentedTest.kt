package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*

class NotificationRestoreFailureInstrumentedTest {
    private suspend fun failedPreflight(newerConnect: Boolean) =
        coroutineScope {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
            val original = AppSession(context)
            val restored = AppSession(context)
            val entered = CompletableDeferred<Unit>()
            val release = CompletableDeferred<Unit>()
            val chosenBound = CompletableDeferred<SessionKey>()
            var failed: ActiveSession? = null
            var push: Deferred<Throwable?>? = null
            var chosen: Deferred<Unit>? = null
            try {
                original.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val saved = checkNotNull(original.active.value)
                val inbox = saved.client.inboxId()
                val group =
                    saved.client.conversations.createGroup(
                        emptyList(),
                        CreateGroupOptions(name = "Saved before preflight"),
                    )
                val groupId = group.id()
                original.signOut()
                original.preferences.setSignedIn(true)
                restored.needsNotificationPreflight = { true }
                restored.stopStoredNotifications = { owner ->
                    failed = owner
                    assertEquals(inbox, owner.client.inboxId())
                    assertEquals(NotificationState.Disabled, owner.client.notificationState())
                    entered.complete(Unit)
                    if (newerConnect) release.await()
                    error("Controlled preflight failure")
                }
                push = async(Dispatchers.IO) { runCatching { restored.restoreForPush() }.exceptionOrNull() }
                withTimeout(30_000) { entered.await() }
                if (newerConnect) {
                    restored.needsNotificationPreflight = { false }
                    restored.beforeBoundConnectOperation = { key -> chosenBound.complete(key) }
                    chosen = async(Dispatchers.IO) { restored.connect(BuildConfig.XMTP_BACKEND_URL, null, false) }
                    withTimeout(30_000) { chosenBound.await() }
                    release.complete(Unit)
                }
                val failure = withTimeout(30_000) { push.await() }
                assertEquals("Controlled preflight failure", failure?.message)
                val closed = checkNotNull(failed)
                assertTrue(
                    runCatching {
                        closed.client.conversations.listGroups(
                            null,
                        )
                    }.exceptionOrNull() is XmtpException.ClientClosed,
                )
                if (newerConnect) {
                    withTimeout(30_000) { chosen?.await() }
                } else {
                    assertNull(restored.active.value)
                    assertTrue(restored.preferences.signedIn())
                    restored.stopStoredNotifications = { owner -> owner.client.disableNotifications() }
                    restored.restore()
                }
                assertNotNull(
                    "The saved or newer intent must open an actual SDK owner after failed preflight",
                    restored.active.value,
                )
                val reopened = checkNotNull(restored.active.value)
                if (newerConnect) assertEquals(chosenBound.await(), reopened.key)
                assertTrue(restored.accepts(reopened.key))
                assertEquals(inbox, reopened.client.inboxId())
                val stored = (reopened.client.conversations.getById(groupId) as Conversation.Group).group
                assertEquals("Saved before preflight", stored.state().name)
                withTimeout(30_000) { restored.connection.first { it == ConnectionState.CONNECTED.toString() } }
                println("PUSH_PREFLIGHT_FAILURE newer=$newerConnect old_client_closed=true actual_owner_recovered=true")
            } finally {
                release.complete(Unit)
                withContext(NonCancellable) {
                    push?.cancelAndJoin()
                    chosen?.cancelAndJoin()
                    restored.beforeBoundConnectOperation = {}
                    restored.needsNotificationPreflight = { false }
                    restored.stopStoredNotifications = {}
                    if (restored.preferences.active() != null) restored.deleteAccount()
                    restored.signOut()
                    original.signOut()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }

    @Test fun failedPushPreflightAllowsActualForegroundRetry() = runBlocking { failedPreflight(false) }

    @Test fun failedPushPreflightPreservesNewerBoundNativeConnect() = runBlocking { failedPreflight(true) }
}
