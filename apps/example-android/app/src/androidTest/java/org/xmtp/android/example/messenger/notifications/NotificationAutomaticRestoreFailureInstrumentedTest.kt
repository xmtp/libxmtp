package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.ExampleApp
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*

class NotificationAutomaticRestoreFailureInstrumentedTest {
    private data class SavedAccount(
        val original: AppSession,
        val profile: BackendProfile,
        val inbox: String,
        val group: String,
        val encryption: ByteArray,
    )

    private suspend fun savedAccount(): SavedAccount {
        val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        (context as ExampleApp).session.awaitStartupExportCleanup()
        val original = AppSession(context)
        original.awaitStartupExportCleanup()
        try {
            println("AUTOMATIC_RESTORE_STAGE seed_native_account")
            original.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val owner = checkNotNull(original.active.value)
            assertEquals(NotificationState.Disabled, owner.client.notificationState())
            val inbox = owner.client.inboxId()
            val group =
                owner.client.conversations.createGroup(
                    emptyList(),
                    CreateGroupOptions(name = "Automatic restore baseline"),
                )
            val encryption = checkNotNull(original.secrets.read(owner.key.profileId, "database-key"))
            val saved = SavedAccount(original, owner.profile, inbox, group.id(), encryption)
            original.signOut()
            original.preferences.setSignedIn(true)
            return saved
        } catch (failure: Throwable) {
            withContext(NonCancellable) {
                try {
                    if (original.preferences.active() != null) original.deleteAccount()
                    original.signOut()
                } catch (cleanupFailure: Throwable) {
                    failure.addSuppressed(cleanupFailure)
                } finally {
                    AndroidStreamLifecycle.enabled = true
                }
            }
            throw failure
        }
    }

    private suspend fun reopened(
        restored: AppSession,
        saved: SavedAccount,
        chosen: SessionKey? = null,
    ) {
        val owner = checkNotNull(restored.active.value)
        assertTrue(restored.accepts(owner.key))
        if (chosen != null) assertEquals(chosen, owner.key)
        assertEquals(saved.profile.id, owner.key.profileId)
        assertEquals(saved.inbox, owner.client.inboxId())
        assertTrue(restored.preferences.signedIn())
        val stored = (owner.client.conversations.getById(saved.group) as Conversation.Group).group
        assertEquals("Automatic restore baseline", stored.state().name)
        assertTrue(owner.paths.database.exists())
    }

    private suspend fun closed(owner: ActiveSession) {
        assertTrue(
            "The failed temporary SDK owner must end before retry or a newer intent",
            runCatching { owner.client.conversations.listGroups(null) }.exceptionOrNull() is XmtpException.ClientClosed,
        )
    }

    private suspend fun cleanup(
        restored: AppSession,
        saved: SavedAccount,
    ) {
        restored.secrets.write(saved.profile.id, "database-key", saved.encryption)
        restored.beforeSessionStopLock = {}
        restored.beforeBoundConnectOperation = {}
        restored.needsNotificationPreflight = { false }
        restored.stopStoredNotifications = {}
        if (restored.preferences.active() != null) restored.deleteAccount()
        restored.signOut()
        saved.original.signOut()
        AndroidStreamLifecycle.enabled = true
    }

    @Test fun failedNativePreflightBuildKeepsTheSavedAccountRetryable() = retry(corruptKey = true)

    @Test fun failedAcceptedPreflightKeepsTheSavedAccountRetryable() = retry(corruptKey = false)

    private fun retry(corruptKey: Boolean) =
        runBlocking<Unit> {
            withTimeout(90_000) {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                val saved = savedAccount()
                val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
                val restored = AppSession(context)
                restored.awaitStartupExportCleanup()
                var failed: ActiveSession? = null
                try {
                    restored.needsNotificationPreflight = { true }
                    if (corruptKey) {
                        val wrong = saved.encryption.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
                        restored.secrets.write(saved.profile.id, "database-key", wrong)
                    } else {
                        restored.stopStoredNotifications = { owner ->
                            failed = owner
                            assertEquals(NotificationState.Disabled, owner.client.notificationState())
                            error("Controlled automatic preflight failure")
                        }
                    }
                    println("AUTOMATIC_RESTORE_STAGE failing_automatic_open native_builder=$corruptKey")
                    val failure = runCatching { restored.restore() }.exceptionOrNull()
                    if (corruptKey) {
                        assertTrue(
                            "The actual SDK must reject the incorrect saved database key",
                            failure is XmtpException,
                        )
                    } else {
                        assertEquals("Controlled automatic preflight failure", failure?.message)
                        closed(checkNotNull(failed))
                    }
                    restored.secrets.write(saved.profile.id, "database-key", saved.encryption)
                    assertNull(restored.active.value)
                    assertTrue(
                        "A current failed automatic open must retain its saved signed-in state for retry",
                        restored.preferences.signedIn(),
                    )
                    assertEquals(saved.profile, restored.preferences.active())
                    assertTrue(restored.preferences.pendingExportCleanup().isEmpty())
                    restored.stopStoredNotifications = { owner -> owner.client.disableNotifications() }
                    println("AUTOMATIC_RESTORE_STAGE retry_native_account")
                    restored.restore()
                    assertNotNull(
                        "A second automatic restore must open the saved native account",
                        restored.active.value,
                    )
                    reopened(restored, saved)
                    println("AUTOMATIC_RESTORE_RETRY native_builder=$corruptKey actual_saved_group=true")
                } finally {
                    withContext(NonCancellable) { cleanup(restored, saved) }
                }
            }
        }

    @Test fun failedAutomaticPreflightKeepsTheNewerBoundNativeConnect() = newerIntent(signOut = false)

    @Test fun failedAutomaticPreflightCannotUndoAnExplicitSignOut() = newerIntent(signOut = true)

    private fun newerIntent(signOut: Boolean) =
        runBlocking<Unit> {
            withTimeout(90_000) {
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                val saved = savedAccount()
                val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
                val restored = AppSession(context)
                restored.awaitStartupExportCleanup()
                val entered = CompletableDeferred<Unit>()
                val release = CompletableDeferred<Unit>()
                val reserved = CompletableDeferred<Unit>()
                val stopLock = CompletableDeferred<Unit>()
                val chosen = CompletableDeferred<SessionKey>()
                var failed: ActiveSession? = null
                var opening: Deferred<Throwable?>? = null
                var next: Deferred<Unit>? = null
                try {
                    restored.needsNotificationPreflight = { true }
                    restored.stopStoredNotifications = { owner ->
                        failed = owner
                        assertEquals(NotificationState.Disabled, owner.client.notificationState())
                        entered.complete(Unit)
                        release.await()
                        error("Controlled superseded automatic preflight")
                    }
                    opening = async(Dispatchers.IO) { runCatching { restored.restore() }.exceptionOrNull() }
                    withTimeout(30_000) { entered.await() }
                    if (signOut) {
                        restored.beforeSessionStopLock = {
                            reserved.complete(Unit)
                            stopLock.await()
                        }
                        next = async(Dispatchers.IO) { restored.signOut() }
                        withTimeout(30_000) { reserved.await() }
                    } else {
                        restored.needsNotificationPreflight = { false }
                        restored.beforeBoundConnectOperation = { key -> chosen.complete(key) }
                        next = async(Dispatchers.IO) { restored.connect(BuildConfig.XMTP_BACKEND_URL, null, false) }
                        withTimeout(30_000) { chosen.await() }
                    }
                    release.complete(Unit)
                    val failure = withTimeout(30_000) { checkNotNull(opening).await() }
                    assertEquals("Controlled superseded automatic preflight", failure?.message)
                    closed(checkNotNull(failed))
                    if (signOut) {
                        assertFalse(
                            "A superseded automatic rollback must not re-sign in before the stop lock is allowed",
                            restored.preferences.signedIn(),
                        )
                        assertNull(restored.active.value)
                        stopLock.complete(Unit)
                        withTimeout(30_000) { checkNotNull(next).await() }
                        assertFalse(restored.preferences.signedIn())
                        restored.restore()
                        assertNull(restored.active.value)
                    } else {
                        withTimeout(30_000) { checkNotNull(next).await() }
                        assertNotNull("Rollback must retain the newer bound native Connect", restored.active.value)
                        reopened(restored, saved, chosen.await())
                    }
                    println("AUTOMATIC_RESTORE_SUPERSEDED explicit_stop=$signOut old_client_closed=true")
                } finally {
                    release.complete(Unit)
                    stopLock.complete(Unit)
                    withContext(NonCancellable) {
                        opening?.cancelAndJoin()
                        next?.cancelAndJoin()
                        cleanup(restored, saved)
                    }
                }
            }
        }
}
