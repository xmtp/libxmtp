package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.UUID

class NotificationRollbackInstrumentedTest {
    private suspend fun failedOpenPreservesStoredRegistration(cold: Boolean) {
        AndroidStreamLifecycle.enabled = false
        resumeStreams()
        val context = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        val original = AppSession(context)
        val restored = AppSession(context)
        var failedOwner: ActiveSession? = null
        var disables = 0
        try {
            original.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
            val baseline = checkNotNull(original.active.value)
            assertTrue(
                baseline.client.conversations
                    .list(null)
                    .isEmpty(),
            )
            val config =
                NotificationConfig(
                    channel =
                        uniffi.xmtp_sdk.NotificationChannel.Http(
                            "https://example.com/no-topics/${UUID.randomUUID()}",
                            SecureRandom().generateSeed(32),
                        ),
                    consentStates = emptyList(),
                    includeWelcomes = false,
                    includeSyncGroups = false,
                    includeCommits = false,
                )
            assertEquals(NotificationState.Enabled, baseline.client.enableNotifications(config))
            assertEquals(NotificationState.Enabled, baseline.client.notificationState())
            val inbox = baseline.client.inboxId()
            original.signOut()
            original.preferences.setSignedIn(true)
            // This registered-account fixture has no Off preflight policy or FCM transport.
            restored.unregisterNotifications = { client ->
                disables += 1
                client.disableNotifications()
            }
            restored.beforeOpeningListener = { owner ->
                failedOwner = owner
                error("Failed opening fixture")
            }
            val error =
                runCatching {
                    if (cold) restored.restoreForPush() else restored.connect(BuildConfig.XMTP_BACKEND_URL, null, false)
                }.exceptionOrNull()
            assertTrue("The private opening must fail at its listener", error is IllegalStateException)
            val failed = checkNotNull(failedOwner)
            assertNull(restored.active.value)
            assertTrue(
                runCatching {
                    failed.client.conversations.listGroups(
                        null,
                    )
                }.exceptionOrNull() is XmtpException.ClientClosed,
            )
            assertEquals("Failed opening must not unregister stored notifications", 0, disables)
            if (cold) assertTrue(restored.preferences.signedIn())
            restored.beforeOpeningListener = {}
            restored.connect(BuildConfig.XMTP_BACKEND_URL, null, false)
            val reopened = checkNotNull(restored.active.value)
            assertEquals(inbox, reopened.client.inboxId())
            assertEquals(
                "Actual saved SDK registration survives reopening",
                NotificationState.Enabled,
                reopened.client.notificationState(),
            )
            restored.signOut()
            assertEquals("Explicit sign-out still disables registration", 1, disables)
            println(
                "NOTIFICATION_ROLLBACK cold=$cold preserved_enabled=true failed_unregisters=0 explicit_unregisters=1",
            )
        } finally {
            withContext(NonCancellable) {
                restored.beforeOpeningListener = {}
                if (restored.preferences.active() != null) restored.deleteAccount()
                restored.signOut()
                original.signOut()
                AndroidStreamLifecycle.enabled = true
            }
        }
    }

    @Test fun failedColdPushOpeningPreservesRealStoredNotificationState() =
        runBlocking {
            failedOpenPreservesStoredRegistration(true)
        }

    @Test fun failedConnectOpeningPreservesRealStoredNotificationState() =
        runBlocking {
            failedOpenPreservesStoredRegistration(false)
        }
}
