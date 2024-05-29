package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiGroupMember
import uniffi.xmtpv3.FfiPermissionLevel

enum class PermissionLevel {
    MEMBER, ADMIN, SUPER_ADMIN
}
class Member(private val ffiMember: FfiGroupMember) {

    val inboxId: String
        get() = ffiMember.inboxId
    val addresses: List<String>
        get() = ffiMember.accountAddresses
    val permissionLevel: PermissionLevel
        get() = when (ffiMember.permissionLevel) {
            FfiPermissionLevel.MEMBER -> PermissionLevel.MEMBER
            FfiPermissionLevel.ADMIN -> PermissionLevel.ADMIN
            FfiPermissionLevel.SUPER_ADMIN -> PermissionLevel.SUPER_ADMIN
        }
}
