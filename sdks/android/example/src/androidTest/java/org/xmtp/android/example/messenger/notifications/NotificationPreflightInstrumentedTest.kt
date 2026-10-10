package org.xmtp.android.example.messenger.notifications

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.net.URI
import java.security.SecureRandom
import java.util.UUID

/** Seed real SDK state through the local backend. No topic or Welcome can target the HTTP recipient. */
class NotificationPreflightInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    private class OffTransport : PushTransport {
        override val configured = false

        override fun requestToken(callback: (String?, Throwable?) -> Unit) = error("Off must not request a token")
    }

    @Test fun offRestoreDisablesStoredStateWithoutRenewalAndRestartsNormalWorkers() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val original = AppSession(context)
            val originalController = NotificationController(context, original, OffTransport())
            val restored = AppSession(context)
            val controller = NotificationController(context, restored, OffTransport())
            var relay: AppOfflineBackendProxy? = AppOfflineBackendProxy(BuildConfig.XMTP_BACKEND_URL)
            var held: ActiveSession? = null
            var unregisterUnavailable = false
            var registrations = 0
            var closeCount = 0
            controller.enableRegistration = { _, _ ->
                registrations += 1
                error("Off must not register")
            }
            try {
                val source = checkNotNull(relay)
                val url = source.url
                val port = URI(url).port
                original.connect(url, "", false)
                val initial = checkNotNull(original.active.value)
                assertTrue(
                    initial.client.conversations
                        .list(null)
                        .isEmpty(),
                )
                val config =
                    NotificationConfig(
                        channel =
                            uniffi.xmtp_sdk.NotificationChannel.Http(
                                "https://example.com/xmtp-messenger-no-topics/${UUID.randomUUID()}",
                                SecureRandom().generateSeed(32),
                            ),
                        consentStates = emptyList(),
                        includeWelcomes = false,
                        includeSyncGroups = false,
                        includeCommits = false,
                    )
                assertEquals(NotificationState.Enabled, initial.client.enableNotifications(config))
                assertEquals(NotificationState.Enabled, initial.client.notificationState())
                assertTrue(config.consentStates!!.isEmpty())
                assertEquals(false, config.includeWelcomes)
                assertEquals(false, config.includeSyncGroups)
                println("PUSH_PREFLIGHT stage=stored-enabled requested_topics=0 welcomes=false sync_groups=false")
                val inbox = initial.client.inboxId()
                original.signOut()
                original.preferences.setSignedIn(true)
                source.close()
                source.assertUnavailable()
                relay = null
                controller.disableRegistration = { client ->
                    try {
                        withTimeout(4_000) { client.disableNotifications() }
                    } catch (
                        error: Exception,
                    ) {
                        unregisterUnavailable = true
                        throw error
                    }
                }
                val stop = restored.stopStoredNotifications
                restored.stopStoredNotifications = { owner ->
                    held = owner
                    val taskRunner =
                        owner.client
                            .options()
                            .workers!!
                            .intervals
                            .single { it.kind == WorkerKind.TASK_RUNNER }
                    assertEquals(false, taskRunner.enabled)
                    assertEquals(NotificationState.Enabled, owner.client.notificationState())
                    stop(owner)
                    assertTrue(unregisterUnavailable)
                    assertEquals(NotificationState.Disabled, owner.client.notificationState())
                    println("PUSH_PREFLIGHT stage=disabled-before-offline-unregister-finish worker_enabled=false")
                }
                val end = restored.beforeEnd
                restored.beforeEnd = { owner ->
                    closeCount += 1
                    end(owner)
                }
                withTimeout(90_000) { restored.restore() }
                val owner = checkNotNull(restored.active.value)
                assertEquals(inbox, owner.client.inboxId())
                assertEquals(NotificationState.Disabled, owner.client.notificationState())
                assertNull(owner.client.options().workers)
                val preflight = checkNotNull(held)
                assertNotSame(preflight.client, owner.client)
                val closed = runCatching { preflight.client.conversations.listGroups(null) }.exceptionOrNull()
                assertTrue("Preflight must end before normal client ownership", closed is XmtpException.ClientClosed)
                assertFalse(preflight.work.coroutineContext[Job]!!.isActive)
                assertEquals(1, closeCount)
                assertEquals(0, registrations)
                assertEquals(0L, controller.permissionRequest.value)
                println("PUSH_PREFLIGHT stage=closed-preflight-normal-worker-options-restored")
                relay = AppOfflineBackendProxy(BuildConfig.XMTP_BACKEND_URL, listenerPort = port)
                val group = withTimeout(30_000) { owner.client.conversations.createGroup(emptyList()) }
                val id = withTimeout(30_000) { group.sendText("Normal chat after notification preflight") }
                val message = checkNotNull(owner.client.conversations.getMessageById(id))
                assertEquals(DeliveryStatus.PUBLISHED, message.deliveryStatus)
                assertEquals(NotificationState.Disabled, owner.client.notificationState())
                assertFalse(group.state().common.notificationsEnabled)
                assertEquals(0, registrations)
                println("PUSH_PREFLIGHT stage=normal-chat-published-notifications-disabled")
            } finally {
                withContext(NonCancellable) {
                    restored.stopStoredNotifications = {}
                    originalController.close()
                    controller.close()
                    restored.deleteAccount()
                    restored.signOut()
                    original.signOut()
                    held?.let { owner ->
                        owner.work.coroutineContext[Job]?.cancelAndJoin()
                        val closed = runCatching { owner.client.conversations.listGroups(null) }.exceptionOrNull()
                        if (closed !is XmtpException.ClientClosed) owner.client.end()
                    }
                    relay?.close()
                    AndroidStreamLifecycle.enabled = true
                }
            }
        }
}
