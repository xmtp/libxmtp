package org.xmtp.android.example.messenger.session

import org.xmtp.android.example.messenger.ActiveSession
import uniffi.xmtp_sdk.ClientEvent
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.SDKClient

/** Fixed observation points. The graph retains the real handlers for its full lifetime. */
class SessionObservers(
    private val beforeHandle: suspend (ActiveSession, Message) -> Unit = { _, _ -> },
    private val afterHandle: suspend (ActiveSession, Message) -> Unit = { _, _ -> },
    private val beforeInvalidate: suspend (ActiveSession) -> Unit = {},
    private val beforeEvent: suspend (ActiveSession, ClientEvent) -> Unit = { _, _ -> },
    private val onSessionInvalidated: () -> Unit = {},
    private val afterAdmissionChanged: (ActiveSession) -> Unit = {},
    private val beforeUnregister: suspend (SDKClient) -> Unit = {},
    private val beforeEnd: suspend (ActiveSession) -> Unit = {},
    private val beforeProfileRemoval: suspend (String) -> Unit = {},
    private val beforeStoredNotificationStop: suspend (ActiveSession) -> Unit = {},
) {
    fun decorateProjection(delegate: SessionProjection): SessionProjection =
        object : SessionProjection {
            override suspend fun handle(
                owner: ActiveSession,
                message: Message,
            ) {
                beforeHandle(owner, message)
                delegate.handle(owner, message)
                afterHandle(owner, message)
            }

            override suspend fun invalidate(owner: ActiveSession) {
                beforeInvalidate(owner)
                delegate.invalidate(owner)
            }

            override suspend fun event(
                owner: ActiveSession,
                event: ClientEvent,
            ) {
                beforeEvent(owner, event)
                delegate.event(owner, event)
            }
        }

    fun decorateLifecycle(delegate: SessionLifecycle): SessionLifecycle =
        object : SessionLifecycle by delegate {
            override fun onAdmissionChanged(owner: ActiveSession) {
                delegate.onAdmissionChanged(owner)
                afterAdmissionChanged(owner)
            }

            override fun onInvalidated() {
                delegate.onInvalidated()
                onSessionInvalidated()
            }

            override suspend fun unregisterNotifications(client: SDKClient) {
                beforeUnregister(client)
                delegate.unregisterNotifications(client)
            }

            override suspend fun beforeEnd(owner: ActiveSession) {
                this@SessionObservers.beforeEnd(owner)
                delegate.beforeEnd(owner)
            }

            override suspend fun beforeProfileRemoval(profileId: String) {
                this@SessionObservers.beforeProfileRemoval(profileId)
                delegate.beforeProfileRemoval(profileId)
            }

            override suspend fun stopStoredNotifications(owner: ActiveSession) {
                beforeStoredNotificationStop(owner)
                delegate.stopStoredNotifications(owner)
            }
        }
}
