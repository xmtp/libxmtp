package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiInboxState

class InboxState(private val ffiInboxState: FfiInboxState) {
    val inboxId: String
        get() = ffiInboxState.inboxId
    val addresses: List<String>
        get() = ffiInboxState.accountAddresses

    val installations: List<Installation>
        get() = ffiInboxState.installations.map { Installation(it) }

    val recoveryAddress: String
        get() = ffiInboxState.recoveryAddress
}
