package org.xmtp.android.library

import uniffi.xmtpv3.FfiAppDataChange
import uniffi.xmtpv3.FfiAppDataChangeCallback
import uniffi.xmtpv3.FfiUnstableChangeCallbacks

/**
 * A change to a group's opaque `appData`, as observed after it was applied.
 */
data class AppDataChange(
    /** The group whose `appData` changed, hex-encoded. */
    val groupId: String,
    /** Value before the change. `null` when nothing was set. */
    val oldValue: String?,
    /** Value after the change. `null` when the field was cleared. */
    val newValue: String?,
)

/**
 * Notified when a processed message changed a group's `appData`.
 *
 * [onAppDataChanged] is awaited before message processing continues, so a
 * semantic merge — including republishing via `Group.updateAppData` — can
 * finish first. It fires for changes this client made as well as remote ones,
 * so the merge must be idempotent.
 */
interface AppDataChangeHandler {
    suspend fun onAppDataChanged(change: AppDataChange)
}

/**
 * Unstable: the set of group-change callbacks to register on a client.
 *
 * Only [appData] exists today. This is one class rather than a bare callback so
 * handlers for the other mutable fields (name, description, image url, admin
 * lists, permissions, disappearing settings) can be added as further defaulted
 * properties — additive for existing callers.
 *
 * Registered at client creation via [ClientOptions.unstableChangeCallbacks],
 * because the changes it reports arrive from the stream and sync paths, where
 * no SDK call is on the stack to carry them.
 *
 * Pre-release: the shape of the payloads and of this class may change without a
 * major version bump until it graduates.
 */
data class UnstableChangeCallbacks(
    val appData: AppDataChangeHandler? = null,
) {
    /**
     * `null` when nothing is registered, so the core never pays for the
     * before/after snapshot on the message-processing path.
     */
    fun toFfi(): FfiUnstableChangeCallbacks? {
        val handler = appData ?: return null
        return FfiUnstableChangeCallbacks(appData = FfiAppDataChangeHandlerBridge(handler))
    }
}

/**
 * Adapts the SDK-level [AppDataChangeHandler] to the generated FFI interface,
 * keeping [FfiAppDataChange] out of the public surface.
 */
private class FfiAppDataChangeHandlerBridge(
    private val handler: AppDataChangeHandler,
) : FfiAppDataChangeCallback {
    override suspend fun onAppDataChanged(change: FfiAppDataChange) {
        handler.onAppDataChanged(
            AppDataChange(
                groupId = change.groupId.toHex(),
                oldValue = change.oldValue,
                newValue = change.newValue,
            ),
        )
    }
}
