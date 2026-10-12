package org.xmtp.android.example.messenger.platform

import org.xmtp.android.example.messenger.AppSession
import org.xmtp.android.example.messenger.app.DestinationOrigin
import org.xmtp.android.example.messenger.app.DestinationScope
import org.xmtp.android.example.messenger.feature.settings.SettingsController
import org.xmtp.android.example.messenger.notifications.NotificationController

/** A launched request survives Activity recreation and can be completed once. */
internal class NotificationPermissionOwner(
    private val session: AppSession,
    private val notifications: NotificationController,
    private val settings: SettingsController,
    private val destinations: DestinationScope,
) {
    private val lock = Any()
    private var launched: EffectIdentity? = null

    fun claim(requestId: Long): EffectIdentity? {
        if (requestId <= 0 || !notifications.configured) return null
        val owner = session.active.value ?: return null
        val origin = destinations.capture(owner)
        var claimed: EffectIdentity? = null
        destinations.admit(origin) {
            synchronized(lock) {
                if (launched == null && notifications.enabled.value &&
                    notifications.permissionRequest.value == requestId
                ) {
                    claimed = EffectIdentity(requestId, owner.key, origin.destinationToken)
                    launched = claimed
                    notifications.permissionRequested()
                }
            }
        }
        return claimed
    }

    fun complete(granted: Boolean) {
        val request = synchronized(lock) { launched.also { launched = null } } ?: return
        destinations.admit(DestinationOrigin(request.sessionKey, request.destinationToken)) {
            settings.permission(granted)
            notifications.permissionChanged()
        }
    }

    fun launchFailed(identity: EffectIdentity) {
        synchronized(lock) { if (launched == identity) launched = null }
    }
}
