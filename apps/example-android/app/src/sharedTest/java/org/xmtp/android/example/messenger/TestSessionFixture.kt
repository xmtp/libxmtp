package org.xmtp.android.example.messenger

import android.content.Context
import org.xmtp.android.example.messenger.notifications.NotificationController
import org.xmtp.android.example.messenger.session.SessionDiagnosticSink
import org.xmtp.android.example.messenger.session.SessionLifecycle
import org.xmtp.android.example.messenger.session.SessionProjection
import uniffi.xmtp_sdk.ClientEvent
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.SDKClient

/** Inject isolated collaborators before the session can start work. */
internal class TestSessionFixture(
    context: Context,
) {
    val projection = TestSessionProjection()
    val lifecycle = TestSessionLifecycle()
    val session =
        AppSession(
            context,
            projectionFactory = { projection },
            lifecycleFactory = { lifecycle },
            diagnostics = SessionDiagnosticSink { _, _ -> },
        )

    fun bindNotifications(controller: NotificationController) {
        check(lifecycle.delegate == null) { "This fixture already owns a notification controller" }
        lifecycle.delegate = controller.lifecycle
    }
}

internal class TestSessionProjection : SessionProjection {
    var onMessage: suspend (ActiveSession, Message) -> Unit = { _, _ -> }
    var onInvalidated: suspend (ActiveSession) -> Unit = {}
    var onEvent: suspend (ActiveSession, ClientEvent) -> Unit = { _, _ -> }

    override suspend fun handle(
        owner: ActiveSession,
        message: Message,
    ) = onMessage(owner, message)

    override suspend fun invalidate(owner: ActiveSession) = onInvalidated(owner)

    override suspend fun event(
        owner: ActiveSession,
        event: ClientEvent,
    ) = onEvent(owner, event)
}

/** Test latches wrap the real notification lifecycle when that contract is under test. */
internal class TestSessionLifecycle : SessionLifecycle {
    var delegate: SessionLifecycle? = null
    var onNotificationAdmissionChanged: (ActiveSession) -> Unit = { owner -> delegate?.onAdmissionChanged(owner) }
    var onSessionInvalidated: () -> Unit = { delegate?.onInvalidated() }
    var unregisterNotifications: suspend (SDKClient) -> Unit = { client -> delegate?.unregisterNotifications(client) }
    var beforeEnd: suspend (ActiveSession) -> Unit = { owner -> delegate?.beforeEnd(owner) }
    var beforeProfileRemoval: suspend (String) -> Unit = { profile -> delegate?.beforeProfileRemoval(profile) }
    var needsNotificationPreflight: suspend (String) -> Boolean = { profile ->
        delegate?.needsNotificationPreflight(profile)
            ?: false
    }
    var stopStoredNotifications: suspend (ActiveSession) -> Unit = { owner -> delegate?.stopStoredNotifications(owner) }

    override fun onAdmissionChanged(owner: ActiveSession) = onNotificationAdmissionChanged(owner)

    override fun onInvalidated() = onSessionInvalidated()

    override suspend fun unregisterNotifications(client: SDKClient) = unregisterNotifications.invoke(client)

    override suspend fun beforeEnd(owner: ActiveSession) = beforeEnd.invoke(owner)

    override suspend fun beforeProfileRemoval(profileId: String) = beforeProfileRemoval.invoke(profileId)

    override suspend fun needsNotificationPreflight(profileId: String) = needsNotificationPreflight.invoke(profileId)

    override suspend fun stopStoredNotifications(owner: ActiveSession) = stopStoredNotifications.invoke(owner)
}
