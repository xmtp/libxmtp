package org.xmtp.android.example.messenger.session

import org.xmtp.android.example.messenger.ActiveSession
import uniffi.xmtp_sdk.ClientEvent
import uniffi.xmtp_sdk.Message
import uniffi.xmtp_sdk.SDKClient

/** Complete required app work before the default reader advances. */
fun interface RequiredMessageHandler {
    suspend fun handle(
        owner: ActiveSession,
        message: Message,
    )
}

/** The process owns projections. Screen replacement does not replace this handler. */
interface SessionProjection : RequiredMessageHandler {
    suspend fun invalidate(owner: ActiveSession)

    suspend fun event(
        owner: ActiveSession,
        event: ClientEvent,
    ) {}
}

/** These hooks are fixed before a client can open. */
interface SessionLifecycle {
    fun onAdmissionChanged(owner: ActiveSession)

    fun onInvalidated()

    suspend fun unregisterNotifications(client: SDKClient)

    suspend fun beforeEnd(owner: ActiveSession)

    suspend fun beforeProfileRemoval(profileId: String)

    suspend fun needsNotificationPreflight(profileId: String): Boolean

    suspend fun stopStoredNotifications(owner: ActiveSession)
}

interface SessionCommands {
    suspend fun restore()

    suspend fun connect(
        backend: String,
        credential: String?,
        allowPrivateNetwork: Boolean,
    )

    suspend fun retryReader()

    suspend fun signOut()

    suspend fun deleteAccount()

    suspend fun resetLegacyAccount(inboxId: String)
}
