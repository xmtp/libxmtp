package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiInstallation
import java.util.Date

class Installation(
    private val ffiInstallation: FfiInstallation,
) {
    val installationId: String
        get() = ffiInstallation.id.toHex()
    val createdAt: Date?
        get() =
            ffiInstallation.clientTimestampNs?.let {
                Date(it.toLong())
            }
}
