package org.xmtp.android.example.messenger.app

import kotlinx.coroutines.flow.StateFlow
import org.xmtp.android.example.messenger.ActiveSession
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.shared.AppRoute

data class DestinationState(
    val route: AppRoute = AppRoute.Connect,
    val token: Long = 0,
)

data class DestinationOrigin(
    val sessionKey: SessionKey,
    val destinationToken: Long,
)

interface DestinationScope {
    val state: StateFlow<DestinationState>

    fun capture(owner: ActiveSession): DestinationOrigin

    fun accepts(origin: DestinationOrigin): Boolean

    fun admit(
        origin: DestinationOrigin,
        change: () -> Unit,
    ): Boolean

    fun navigate(route: AppRoute)
}
