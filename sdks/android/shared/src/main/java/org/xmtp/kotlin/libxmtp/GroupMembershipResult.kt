package org.xmtp.kotlin.libxmtp

import org.xmtp.kotlin.InboxId
import org.xmtp.kotlin.toHex
import uniffi.xmtpv3.FfiUpdateGroupMembershipResult

class GroupMembershipResult(
    private val ffiUpdateGroupMembershipResult: FfiUpdateGroupMembershipResult,
) {
    val addedMembers: List<InboxId>
        get() = ffiUpdateGroupMembershipResult.addedMembers.map { it.key }
    val removedMembers: List<InboxId>
        get() = ffiUpdateGroupMembershipResult.removedMembers
    val failedInstallationIds: List<String>
        get() = ffiUpdateGroupMembershipResult.failedInstallations.map { it.toHex() }
}
