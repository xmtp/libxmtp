package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.InboxId
import uniffi.xmtpv3.FfiInboxState
import uniffi.xmtpv3.FfiSignatureKind

typealias SignatureKind = FfiSignatureKind

class InboxState(
        private val ffiInboxState: FfiInboxState,
) {
    val inboxId: InboxId
        get() = ffiInboxState.inboxId
    val identities: List<PublicIdentity>
        get() = ffiInboxState.accountIdentities.map { PublicIdentity(it) }

    val installations: List<Installation>
        get() = ffiInboxState.installations.map { Installation(it) }

    val recoveryPublicIdentity: PublicIdentity
        get() = PublicIdentity(ffiInboxState.recoveryIdentity)

    val creationSignatureKind: SignatureKind?
        get() = ffiInboxState.creationSignatureKind
}
