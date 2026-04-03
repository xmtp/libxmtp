package org.xmtp.kotlin.libxmtp

import uniffi.xmtpv3.FfiMessageDisappearingSettings

class DisappearingMessageSettings(
    val disappearStartingAtNs: Long,
    val retentionDurationInNs: Long,
) {
    companion object {
        fun createFromFfi(ffiSettings: FfiMessageDisappearingSettings): DisappearingMessageSettings =
            DisappearingMessageSettings(ffiSettings.fromNs, ffiSettings.inNs)
    }
}
