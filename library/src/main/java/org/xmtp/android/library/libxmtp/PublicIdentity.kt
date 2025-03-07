package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiIdentifier
import uniffi.xmtpv3.FfiIdentifierKind

enum class IdentityKind {
    ETHEREUM, PASSKEY
}

class PublicIdentity(val ffiPrivate: FfiIdentifier) {
    constructor(
        kind: IdentityKind,
        identifier: String,
        relyingPartner: String? = null,
    ) :
        this(
            ffiPrivate = FfiIdentifier(
                identifier,
                kind.toFfiPublicIdentifierKind(),
                relyingPartner
            ),
        )

    val kind: IdentityKind
        get() = ffiPrivate.identifierKind.toIdentityKind()

    val identifier: String
        get() = ffiPrivate.identifier

    val relyingPartner: String?
        get() = ffiPrivate.relyingPartner
}

fun IdentityKind.toFfiPublicIdentifierKind(): FfiIdentifierKind {
    return when (this) {
        IdentityKind.ETHEREUM -> FfiIdentifierKind.ETHEREUM
        IdentityKind.PASSKEY -> FfiIdentifierKind.PASSKEY
    }
}

fun FfiIdentifierKind.toIdentityKind(): IdentityKind {
    return when (this) {
        FfiIdentifierKind.ETHEREUM -> IdentityKind.ETHEREUM
        FfiIdentifierKind.PASSKEY -> IdentityKind.PASSKEY
    }
}
