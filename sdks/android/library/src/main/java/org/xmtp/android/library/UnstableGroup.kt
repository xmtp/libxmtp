package org.xmtp.android.library

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiEnableProposalsOptions

/**
 * Pre-release ("unstable") surface for a [Group], reached through
 * `group.unstable`.
 *
 * Everything here is unstable: the API shape may still change and, in
 * some cases (see [enableProposals]), the effect is one-way and
 * irreversible. Adding a function here needs no per-function annotation —
 * the class-level `@UnstableApi` covers every member, and the opt-in
 * requirement follows an `UnstableGroup` instance even if it escapes the
 * `group.unstable` accessor. When an API graduates it moves onto [Group]
 * directly and is removed here, so callers of the `unstable` form get a
 * compile-time break to migrate against.
 */
@UnstableApi(
    "APIs on Group.unstable are pre-release: shapes may change and some (e.g. enableProposals) are one-way and irreversible.",
)
class UnstableGroup(
    private val libXMTPGroup: FfiConversation,
) {
    /**
     * Migrate this group's metadata from the legacy GroupContextExtensions
     * shape onto OpenMLS `AppDataUpdate` proposals. After this returns
     * successfully, subsequent metadata writes (group name, description,
     * image URL, admin list, permissions) flow through the proposal-based
     * path instead of GCE commits.
     *
     * @param force Skip the key-package capability check. Before you set
     *   this to `true`, verify that every member supports proposals and
     *   has a version at or above `minVersion`. Defaults to `false`.
     * @param minVersion Override the `MIN_SUPPORTED_PROTOCOL_VERSION`
     *   floor. `null` defaults to libxmtp's
     *   `PROPOSALS_MIN_PROTOCOL_VERSION` — the release where proposals
     *   support first ships.
     *
     * Hard-fails if `force == false` and any member's latest key
     * package doesn't advertise `ProposalType.AppDataUpdate`. The
     * migration is one-way — a migrated group cannot return to the
     * legacy path.
     */
    suspend fun enableProposals(
        force: Boolean = false,
        minVersion: String? = null,
    ) = withContext(Dispatchers.IO) {
        try {
            libXMTPGroup.enableProposals(
                FfiEnableProposalsOptions(force = force, minVersion = minVersion),
            )
        } catch (e: Exception) {
            throw XMTPException("Unable to enable proposals on group", e)
        }
    }
}
