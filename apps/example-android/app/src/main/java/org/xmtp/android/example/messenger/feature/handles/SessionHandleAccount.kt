package org.xmtp.android.example.messenger.feature.handles

import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.feature.AppFeaturePreferences
import org.xmtp.android.example.shared.feature.handles.HandleMode
import uniffi.xmtp_sdk.*

/** Binds the handle editor to one session and the destination it opened on. */
internal class SessionHandleAccount(
    private val owner: ActiveSession,
    private val preferences: AppFeaturePreferences,
    private val step: HandleStep,
    private val destinations: DestinationScope,
    private val origin: DestinationOrigin,
    private val mode: HandleMode,
    private val leave: () -> Unit,
) : HandleAccount {
    override suspend fun handles(): OwnHandles {
        val state = owner.client.inboxState(false)
        return OwnHandles(state.handle, state.otherHandles)
    }

    override fun minNameLength(): Int =
        owner.client
            .serverConfiguration()
            .handles.minNameLength
            .toInt()

    override suspend fun availableHandles(name: String): List<String> = owner.client.availableHandles(name)

    override suspend fun claimHandle(handle: String) = owner.client.claimHandle(handle)

    override fun claimTimes(): Map<String, Long> = preferences.handleClaims(owner.key.profileId)

    override fun recordClaim(
        handle: String,
        atMs: Long,
    ) {
        preferences.recordHandleClaim(owner.key, handle, atMs)
    }

    override fun finish() {
        step.finish(owner.key, mode)
        destinations.admit(origin) { leave() }
    }
}
