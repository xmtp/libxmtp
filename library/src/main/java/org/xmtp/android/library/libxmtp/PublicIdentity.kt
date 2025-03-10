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
    ) :
        this(
            ffiPrivate = FfiIdentifier(
                identifier,
                kind.toFfiPublicIdentifierKind(),
            ),
        )

    val kind: IdentityKind
        get() = ffiPrivate.identifierKind.toIdentityKind()

    val identifier: String
        get() = ffiPrivate.identifier.lowercase()
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
