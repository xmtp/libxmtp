package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiGroupMembershipCapabilities
import uniffi.xmtpv3.FfiInboxCapabilities
import uniffi.xmtpv3.FfiInstallationCapabilities
import uniffi.xmtpv3.FfiMlsExtensionType

/**
 * An MLS extension type advertised by an installation's key package or
 * present in a group's context.
 *
 * Mirrors the libxmtp `MlsExtensionType`; forward/unknown types are preserved
 * verbatim. This is a generic capability primitive — filter it to the question
 * you care about (e.g. the proposal migration is concerned with
 * [AppDataDictionary]).
 */
sealed class MlsExtensionType {
    object ApplicationId : MlsExtensionType()

    object RatchetTree : MlsExtensionType()

    object RequiredCapabilities : MlsExtensionType()

    object ExternalPub : MlsExtensionType()

    object ExternalSenders : MlsExtensionType()

    object LastResort : MlsExtensionType()

    object ImmutableMetadata : MlsExtensionType()

    object AppDataDictionary : MlsExtensionType()

    data class Unknown(
        val id: UShort,
    ) : MlsExtensionType()

    data class Grease(
        val id: UShort,
    ) : MlsExtensionType()

    internal companion object {
        fun fromFfi(ffi: FfiMlsExtensionType): MlsExtensionType =
            when (ffi) {
                is FfiMlsExtensionType.ApplicationId -> ApplicationId
                is FfiMlsExtensionType.RatchetTree -> RatchetTree
                is FfiMlsExtensionType.RequiredCapabilities -> RequiredCapabilities
                is FfiMlsExtensionType.ExternalPub -> ExternalPub
                is FfiMlsExtensionType.ExternalSenders -> ExternalSenders
                is FfiMlsExtensionType.LastResort -> LastResort
                is FfiMlsExtensionType.ImmutableMetadata -> ImmutableMetadata
                is FfiMlsExtensionType.AppDataDictionary -> AppDataDictionary
                is FfiMlsExtensionType.Unknown -> Unknown(ffi.id)
                is FfiMlsExtensionType.Grease -> Grease(ffi.id)
            }
    }
}

/**
 * Capabilities for a single installation (device) of a member.
 */
class InstallationCapabilities(
    private val ffi: FfiInstallationCapabilities,
) {
    /** The installation (device) key. */
    val installationId: ByteArray
        get() = ffi.installationId

    /** True for the local (this device's) installation. */
    val isOwn: Boolean
        get() = ffi.isOwn

    /**
     * The MLS extension types this installation advertises. Empty when
     * [capabilitiesKnown] is false.
     */
    val supportedExtensions: List<MlsExtensionType>
        get() = ffi.supportedExtensions.map { MlsExtensionType.fromFfi(it) }

    /**
     * Whether capabilities were determined. `false` means the key package
     * couldn't be fetched or failed verification — distinct from an
     * installation that advertises no extensions.
     */
    val capabilitiesKnown: Boolean
        get() = ffi.capabilitiesKnown
}

/**
 * Per-inbox installation capabilities. Map [inboxId] to a profile to attribute
 * capabilities to a person.
 */
class InboxCapabilities(
    private val ffi: FfiInboxCapabilities,
) {
    val inboxId: String
        get() = ffi.inboxId

    val installations: List<InstallationCapabilities>
        get() = ffi.installations.map { InstallationCapabilities(it) }
}

/**
 * A generic membership/capability snapshot for a group.
 *
 * Reports raw facts rather than answers. For the proposal (app-data-dictionary)
 * migration specifically: the group is already migrated when [contextExtensions]
 * contains [MlsExtensionType.AppDataDictionary], it's eligible to migrate when
 * every installation's [InstallationCapabilities.supportedExtensions] contains
 * it, and the inboxes blocking migration are those with an installation that
 * doesn't.
 *
 * Read it with [org.xmtp.android.library.Group.membershipCapabilities]; drive
 * the upgrade with [org.xmtp.android.library.Group.enableProposals].
 */
class GroupMembershipCapabilities(
    private val ffi: FfiGroupMembershipCapabilities,
) {
    /** Extension types present in the group's context. */
    val contextExtensions: List<MlsExtensionType>
        get() = ffi.contextExtensions.map { MlsExtensionType.fromFfi(it) }

    /** Per-inbox, per-installation capability breakdown — one entry per member inbox, in no particular order. */
    val members: List<InboxCapabilities>
        get() = ffi.members.map { InboxCapabilities(it) }
}
