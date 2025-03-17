package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.InboxId
import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiUpdateGroupMembershipResult

class GroupMembershipResult(private val ffiUpdateGroupMembershipResult: FfiUpdateGroupMembershipResult) {
    val addedMembers: List<InboxId>
        get() = ffiUpdateGroupMembershipResult.addedMembers.map { it.key }
    val removedMembers: List<InboxId>
        get() = ffiUpdateGroupMembershipResult.removedMembers
    val failedInstallationIds: List<String>
        get() = ffiUpdateGroupMembershipResult.failedInstallations.map { it.toHex() }
}
