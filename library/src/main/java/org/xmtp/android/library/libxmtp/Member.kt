package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiGroupMember

class Member(private val ffiMember: FfiGroupMember) {

    val inboxId: String
        get() = ffiMember.inboxId
    val addresses: List<String>
        get() = ffiMember.accountAddresses
}
