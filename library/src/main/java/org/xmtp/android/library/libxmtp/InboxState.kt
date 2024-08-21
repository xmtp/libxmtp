package uniffi.xmtpv3.org.xmtp.android.library.libxmtp

import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiInboxState

class InboxState(private val ffiInboxState: FfiInboxState) {
    val inboxId: String
        get() = ffiInboxState.inboxId
    val addresses: List<String>
        get() = ffiInboxState.accountAddresses

    val installationIds: List<String>
        get() = ffiInboxState.installationIds.map { it.toHex() }

    val recoveryAddress: String
        get() = ffiInboxState.recoveryAddress
}
