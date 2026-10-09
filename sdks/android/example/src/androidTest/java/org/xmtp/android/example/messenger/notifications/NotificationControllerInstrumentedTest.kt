package org.xmtp.android.example.messenger.notifications

import android.Manifest
import android.app.Notification
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig
import org.xmtp.android.example.messenger.*
import uniffi.xmtp_sdk.*
import java.security.SecureRandom
import java.util.Base64
import java.util.UUID

/** Test native admission with simulated FCM registration and a captured publisher. */
class NotificationControllerInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    private fun payload(
        kind: Int,
        id: String,
        sequence: String = "1",
    ) = mapOf(
        "topic" to
            Base64
                .getEncoder()
                .encodeToString(
                    byteArrayOf(kind.toByte()) +
                        id
                            .chunked(2)
                            .map {
                                it
                                    .toInt(16)
                                    .toByte()
                            }.toByteArray(),
                ),
        "sequence_id" to sequence,
    )

    private fun installation(owner: ActiveSession) =
        owner.client
            .installationIdBytes()
            .joinToString("") { "%02x".format(it.toInt() and 255) }

    private class Transport(
        override val configured: Boolean = true,
    ) : PushTransport {
        var requests = 0

        override fun requestToken(callback: (String?, Throwable?) -> Unit) {
            requests += 1
        }
    }

    private fun controller(
        session: AppSession,
        transport: Transport = Transport(),
    ) = NotificationController(
        context,
        session,
        transport,
    ).also {
        it.permissionGranted = { true }
        it.enableRegistration = { _, _ -> NotificationState.Enabled }
        it.disableRegistration = { }
        // Supply only the unavailable FCM registration state. Consent and membership remain native reads.
        it.readConversationState = { chat -> conversationState(chat).copy(notificationsEnabled = true) }
        it.tokenChanged("compile-test-token")
    }

    private suspend fun enabled(
        session: AppSession,
        controller: NotificationController,
    ) {
        val owner = checkNotNull(session.active.value)
        owner.work.async { controller.setEnabled(owner, true) }.await()
    }

    private suspend fun <T> stage(
        session: AppSession,
        name: String,
        block: suspend () -> T,
    ): T =
        try {
            withTimeout(30_000) { block() }
        } catch (error: TimeoutCancellationException) {
            throw AssertionError(
                "Stage: $name; connection=${session.connection.value}; reader=${session.readerError.value}",
                error,
            )
        }

    private suspend fun cleanup(
        session: AppSession,
        controller: NotificationController? = null,
    ) {
        withContext(NonCancellable) {
            controller?.close()
            session.beforeOpeningListener = {}
            if (session.preferences.active() != null) session.deleteAccount()
            session.signOut()
        }
    }

    @Test fun unconfiguredControllerDoesNotRequestTokenPermissionOrRegistration() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val transport = Transport(false)
            val controller = controller(session, transport)
            var registrations = 0
            controller.enableRegistration = { _, _ ->
                registrations += 1
                NotificationState.Enabled
            }
            controller.disableRegistration = { registrations += 1 }
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                if (Build.VERSION.SDK_INT >= 33 &&
                    ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
                    PackageManager.PERMISSION_GRANTED
                ) {
                    // The outer seam admits the route. The actual publisher must reject Android denial.
                    assertFalse(
                        controller.postIfCurrent(
                            PushOwner(owner.key.profileId, owner.key.generation, installation(owner)),
                            checkNotNull(parsePush(payload(1, installation(owner)))),
                            PushRoute(owner.key.profileId, null),
                        ),
                    )
                }
                controller.permissionGranted = { false }
                enabled(session, controller)
                controller.tokenChanged("changed")
                owner.work.async { controller.refresh(owner) }.await()
                assertFalse(controller.receive(payload(1, installation(owner))))
                session.signOut()
                assertEquals(0, transport.requests)
                assertEquals(0, registrations)
                assertEquals(0L, controller.permissionRequest.value)
                assertFalse(controller.enabled.value)
                assertEquals("Off: Firebase is not configured", controller.status.value)
            } finally {
                cleanup(session, controller)
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun nativeRoutesRecheckConsentMuteAndGenericContent() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val controller = controller(session)
            val posted = linkedMapOf<String, Pair<PushRoute, Notification>>()
            // Capture construction and routes. This fixture does not post to Android.
            controller.postNotification = { envelope, route, notification ->
                posted[envelope.tag] = route to notification
                true
            }
            var peer: SDKClient? = null
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                enabled(session, controller)
                val group = owner.client.conversations.createGroup(emptyList())
                peer =
                    SDKClient.create(
                        context,
                        localSignerFromPrivateKey(SecureRandom().generateSeed(32)),
                        ClientOptions(
                            backend =
                                BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)),
                            storage = StorageOptions(location = StorageLocation.InMemory),
                            deviceSync = false,
                        ),
                    )
                val dm = owner.client.conversations.createDm(checkNotNull(peer).inboxId())
                assertTrue(controller.receive(payload(0, group.id())))
                assertTrue(controller.receive(payload(0, group.id())))
                assertEquals(1, posted.size)
                assertTrue(controller.receive(payload(0, group.id(), "2")))
                assertTrue(controller.receive(payload(0, dm.id())))
                assertEquals(
                    dm.id(),
                    posted.values
                        .last()
                        .first.conversation,
                )
                assertFalse(controller.receive(payload(0, "11".repeat(16))))
                group.updateConsentState(ConsentState.DENIED)
                assertFalse(controller.receive(payload(0, group.id(), "3")))
                group.updateConsentState(ConsentState.ALLOWED)
                controller.setConversationEnabled(owner, Conversation.Group(group), false)
                assertFalse(controller.receive(payload(0, group.id(), "3")))
                controller.setConversationEnabled(owner, Conversation.Group(group), true)
                var firstRead = true
                controller.readConversationState = { chat ->
                    val state = conversationState(chat).copy(notificationsEnabled = true)
                    if (firstRead) {
                        firstRead = false
                        chat.updateConsentState(ConsentState.DENIED)
                    }
                    state
                }
                assertFalse(controller.receive(payload(0, group.id(), "3")))
                controller.readConversationState = { chat ->
                    conversationState(chat).copy(notificationsEnabled = true)
                }
                group.updateConsentState(ConsentState.ALLOWED)
                val route = posted.values.first().first
                assertEquals(
                    route,
                    controller.tap(
                        Intent()
                            .putExtra(
                                "push-profile",
                                owner.key.profileId,
                            ).putExtra(
                                "push-conversation",
                                group.id(),
                            ),
                    ),
                )
                group.updateConsentState(ConsentState.DENIED)
                assertNull(
                    controller.tap(
                        Intent()
                            .putExtra(
                                "push-profile",
                                owner.key.profileId,
                            ).putExtra(
                                "push-conversation",
                                group.id(),
                            ),
                    ),
                )
                for ((_, notification) in posted.values) {
                    assertEquals("XMTP Messenger", notification.extras.getString(Notification.EXTRA_TITLE))
                    assertEquals("You got a message.", notification.extras.getString(Notification.EXTRA_TEXT))
                    val publicBody = notification.publicVersion.extras.getString(Notification.EXTRA_TEXT)
                    assertEquals("You got a message.", publicBody)
                }
                owner.work.async { controller.setEnabled(owner, false) }.await()
                assertFalse(controller.receive(payload(1, installation(owner))))
                val snapshot = checkNotNull(controller.current())
                val delayed = checkNotNull(parsePush(payload(0, group.id(), "9")))
                session.onSessionInvalidated = {
                    assertSame(owner, session.active.value)
                    assertFalse(controller.postIfCurrent(snapshot, delayed, route))
                    runBlocking { assertNull(session.restoreForPush()) }
                    assertSame(owner, session.active.value)
                }
                session.signOut()
                assertEquals(3, posted.size)
            } finally {
                session.onSessionInvalidated = {}
                withContext(NonCancellable) { peer?.end() }
                cleanup(session, controller)
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun failedUnregisterAndColdProfileSwitchDropOldNativeTopics() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val controller = controller(session)
            var recreated: AppSession? = null
            var cold: NotificationController? = null
            var aProfile: BackendProfile? = null
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val a = checkNotNull(session.active.value)
                aProfile = a.profile
                val aGroup =
                    a.client.conversations
                        .createGroup(emptyList())
                        .id()
                val aWelcome = installation(a)
                enabled(session, controller)
                controller.disableRegistration = { error("Unregister failed") }
                session.signOut()
                assertFalse(session.preferences.signedIn())
                assertFalse(controller.receive(payload(0, aGroup)))
                assertFalse(controller.receive(payload(1, aWelcome)))
                assertNull(AppSession(context).restoreForPush())
                val local = java.net.URI(BuildConfig.XMTP_BACKEND_URL)
                session.connect("http://127.0.0.1:${local.port}", "", false)
                val b = checkNotNull(session.active.value)
                val bGroup =
                    b.client.conversations
                        .createGroup(emptyList())
                        .id()
                enabled(session, controller)
                session.signOut()
                // The client is ended. Restore the saved signed-in bit to model process recreation.
                session.preferences.setSignedIn(true)
                recreated = AppSession(context)
                cold = controller(checkNotNull(recreated))
                val posted = mutableListOf<PushRoute>()
                cold.postNotification = { _, route, _ ->
                    posted += route
                    true
                }
                assertFalse(cold.receive(payload(0, aGroup)))
                assertFalse(cold.receive(payload(1, aWelcome)))
                assertTrue(cold.receive(payload(0, bGroup)))
                val restored = checkNotNull(recreated.active.value)
                assertTrue(cold.receive(payload(1, installation(restored))))
                assertNull(posted.last().conversation)
                assertTrue(posted.all { it.profile == b.key.profileId })
            } finally {
                cleanup(recreated ?: session, cold)
                controller.close()
                aProfile?.let { profile ->
                    session.preferences.setActive(profile)
                    session.deleteAccount()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun coldRestoreHasNoDefaultReaderAndForegroundAdoptsTheSameOwner() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val original = AppSession(context)
            val reopened = AppSession(context)
            var probe: Job? = null
            try {
                original.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val before = checkNotNull(original.active.value)
                val inbox = before.client.inboxId()
                original.signOut()
                original.preferences.setSignedIn(true)
                val cold = checkNotNull(reopened.restoreForPush())
                assertEquals(inbox, cold.client.inboxId())
                val probeFailure = CompletableDeferred<Throwable>()
                probe =
                    launch(Dispatchers.IO) {
                        try {
                            cold.client.conversations
                                .streamAllMessages()
                                .collect { }
                        } catch (
                            error: Throwable,
                        ) {
                            probeFailure.complete(error)
                        }
                    }
                delay(500)
                assertFalse("A cold push must not hold the default reader", probeFailure.isCompleted)
                assertNull(reopened.readerError.value)
                probe.cancelAndJoin()
                probe = null
                reopened.restore()
                assertSame(cold, reopened.active.value)
                println("PUSH_RESTORE_ADOPT stage=same-owner-reader-start")
                stage(reopened, "foreground reader connected") {
                    reopened.connection.first { it == ConnectionState.CONNECTED.toString() }
                }
                val failure =
                    stage(reopened, "foreground owns default reader") {
                        runCatching {
                            cold.client.conversations
                                .streamAllMessages()
                                .collect { }
                        }.exceptionOrNull()
                    }
                val detail = "The foreground owner must hold the default reader: $failure"
                assertTrue(detail, failure is XmtpException.ConsumerOwned)
            } finally {
                probe?.cancelAndJoin()
                cleanup(reopened)
                original.signOut()
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun coldRestoreRejectsMissingDatabaseSignedOutAndResetAndClosesFailedOpening() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            var session = AppSession(context)
            var controller = controller(session, Transport(false))
            val notificationPreferences = NotificationPreferences(context)
            var failedOwner: ActiveSession? = null
            try {
                val absent =
                    BackendProfile(
                        UUID
                            .randomUUID()
                            .toString(),
                        BuildConfig.XMTP_BACKEND_URL,
                        "11".repeat(32),
                        "0x" + "11".repeat(20),
                    )
                session.preferences.setActive(absent)
                notificationPreferences.setEnabled(absent.id, true)
                session.preferences.setSignedIn(false)
                assertNull(session.restoreForPush())
                session.preferences.setSignedIn(true)
                assertNull(session.restoreForPush())
                assertFalse(absent.paths(context.filesDir).database.exists())
                session.preferences.saveReset(absent.paths(context.filesDir).resetRecord(absent.id))
                assertNull(session.restoreForPush())
                session.deleteAccount()
                assertFalse(notificationPreferences.enabled(absent.id))
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                session.signOut()
                session.preferences.setSignedIn(true)
                controller.close()
                session = AppSession(context)
                controller = controller(session, Transport(false))
                session.beforeOpeningListener = { owner ->
                    failedOwner = owner
                    error("Push listener failure")
                }
                assertNotNull(runCatching { session.restoreForPush() }.exceptionOrNull())
                val failed = checkNotNull(failedOwner)
                assertNull(session.active.value)
                val error = runCatching { failed.client.conversations.listGroups(null) }.exceptionOrNull()
                val detail = "Failed push opening must end its native client: $error"
                assertTrue(detail, error is XmtpException.ClientClosed)
                assertFalse(failed.work.coroutineContext[Job]!!.isActive)
            } finally {
                withContext(NonCancellable) {
                    failedOwner?.let { held ->
                        held.work.coroutineContext[Job]?.cancelAndJoin()
                        val closed = runCatching { held.client.conversations.listGroups(null) }.exceptionOrNull()
                        if (closed !is XmtpException.ClientClosed) held.client.end()
                    }
                }
                cleanup(session, controller)
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun controllerReconcilesTokensPermissionAndSdkFailure() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val controller = controller(session)
            val configurations = mutableListOf<NotificationConfig>()
            var sdkState: NotificationState = NotificationState.Disabled
            var disables = 0
            controller.readRegistrationState = { sdkState }
            controller.enableRegistration = { _, config ->
                configurations += config
                sdkState = NotificationState.Enabled
                sdkState
            }
            controller.disableRegistration = {
                disables += 1
                sdkState = NotificationState.Disabled
            }
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                enabled(session, controller)
                val first = configurations.single()
                assertEquals(listOf(ConsentState.ALLOWED, ConsentState.UNKNOWN), first.consentStates)
                assertEquals(true, first.includeWelcomes)
                assertEquals(false, first.includeSyncGroups)
                assertEquals(false, first.includeCommits)
                assertEquals("compile-test-token", (first.channel as uniffi.xmtp_sdk.NotificationChannel.Fcm).token)
                controller.tokenChanged("new-token")
                owner.work.async { controller.refresh(owner) }.await()
                assertEquals(2, configurations.size)
                val second = configurations.last().channel as uniffi.xmtp_sdk.NotificationChannel.Fcm
                assertEquals("new-token", second.token)
                controller.permissionGranted = { false }
                owner.work.async { controller.refresh(owner) }.await()
                assertEquals(1, disables)
                assertEquals("Off: Android permission is required", controller.status.value)
                controller.permissionGranted = { true }
                controller.enableRegistration = { _, _ ->
                    sdkState = NotificationState.Failed(NotificationFailure.CHANNEL_NOT_CONFIGURED)
                    sdkState
                }
                owner.work.async { controller.refresh(owner) }.await()
                assertEquals("Backend FCM channel is not configured", controller.status.value)
                owner.work.async { controller.setEnabled(owner, false) }.await()
                assertEquals("Off", controller.status.value)
            } finally {
                cleanup(session, controller)
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun signOutCancelsOwnedEnableBeforeItsFinalUnregister() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            val controller = controller(session)
            val entered = CompletableDeferred<Unit>()
            val cancelled = CompletableDeferred<Unit>()
            val order = mutableListOf<String>()
            try {
                session.connect(BuildConfig.XMTP_BACKEND_URL, "", false)
                val owner = checkNotNull(session.active.value)
                controller.enableRegistration = { _, _ ->
                    entered.complete(Unit)
                    try {
                        awaitCancellation()
                    } finally {
                        order += "enable-stopped"
                        cancelled.complete(Unit)
                    }
                }
                controller.disableRegistration = {
                    assertFalse(session.preferences.signedIn())
                    assertTrue(cancelled.isCompleted)
                    order += "unregistered"
                }
                owner.work.launch { controller.setEnabled(owner, true) }
                withTimeout(30_000) { entered.await() }
                withTimeout(30_000) { session.signOut() }
                assertEquals(listOf("enable-stopped", "unregistered"), order)
                assertNull(session.active.value)
            } finally {
                cleanup(session, controller)
                AndroidStreamLifecycle.enabled = true
            }
        }
}
