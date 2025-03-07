package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.ConsentState
import org.xmtp.android.library.InboxId
import uniffi.xmtpv3.FfiConversationMember
import uniffi.xmtpv3.FfiPermissionLevel

enum class PermissionLevel {
    MEMBER, ADMIN, SUPER_ADMIN
}
class Member(private val ffiMember: FfiConversationMember) {

    val inboxId: InboxId
        get() = ffiMember.inboxId
    val identities: List<PublicIdentity>
        get() = ffiMember.accountIdentifiers.map { PublicIdentity(it) }
    val permissionLevel: PermissionLevel
        get() = when (ffiMember.permissionLevel) {
            FfiPermissionLevel.MEMBER -> PermissionLevel.MEMBER
            FfiPermissionLevel.ADMIN -> PermissionLevel.ADMIN
            FfiPermissionLevel.SUPER_ADMIN -> PermissionLevel.SUPER_ADMIN
        }

    val consentState: ConsentState
        get() = ConsentState.fromFfiConsentState(ffiMember.consentState)
}
