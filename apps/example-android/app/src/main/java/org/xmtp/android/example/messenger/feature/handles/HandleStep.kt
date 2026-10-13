package org.xmtp.android.example.messenger.feature.handles

import kotlinx.coroutines.CancellationException
import org.xmtp.android.example.messenger.SessionKey
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.feature.handles.HandleMode

/** The profile flag that ends Choose your handle for one account. */
interface HandleStepStore {
    /** True after the account claimed or skipped a handle in onboarding. */
    fun handleStepDone(profile: String): Boolean

    fun finishHandleStep(key: SessionKey): Boolean
}

/**
 * Choose your handle runs once for each account (D10). It opens for an inbox with no handle
 * until the user claims a handle or taps Skip for now. Tests that start from Chats turn [offer] off.
 */
internal class HandleStep(
    private val store: HandleStepStore,
    private val offer: Boolean = true,
) {
    /**
     * The first route of the account. [handle] reads the current handle from the inbox state
     * that this device holds. If that read fails, Chats opens, because handles are optional.
     */
    suspend fun start(
        key: SessionKey,
        handle: suspend () -> String?,
    ): AppRoute {
        if (!offer || store.handleStepDone(key.profileId)) return AppRoute.Chats
        val current =
            try {
                handle()
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                return AppRoute.Chats
            }
        if (current == null) return AppRoute.ChooseHandle
        // The inbox claimed a handle on another device, so it never needs the step here.
        store.finishHandleStep(key)
        return AppRoute.Chats
    }

    /** A claim or Skip for now ends the onboarding step. Change handle in Settings does not change it. */
    fun finish(
        key: SessionKey,
        mode: HandleMode,
    ) {
        if (mode == HandleMode.CHOOSE) store.finishHandleStep(key)
    }
}
