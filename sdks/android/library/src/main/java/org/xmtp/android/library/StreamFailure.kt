package org.xmtp.android.library

import uniffi.xmtpv3.FfiException
import uniffi.xmtpv3.FfiStreamBarrierCause
import uniffi.xmtpv3.FfiStreamBarrierCauseKind
import uniffi.xmtpv3.FfiStreamBarrierFailure
import uniffi.xmtpv3.FfiStreamBarrierReason
import uniffi.xmtpv3.FfiStreamBarrierTopic
import uniffi.xmtpv3.FfiStreamFailureDetails
import uniffi.xmtpv3.FfiStreamFailureKind
import uniffi.xmtpv3.getStreamFailureDetails

typealias StreamFailureKind = FfiStreamFailureKind
typealias StreamBarrierReason = FfiStreamBarrierReason
typealias StreamBarrierCauseKind = FfiStreamBarrierCauseKind
typealias StreamBarrierCause = FfiStreamBarrierCause
typealias StreamBarrierTopic = FfiStreamBarrierTopic
typealias StreamBarrierFailure = FfiStreamBarrierFailure
typealias StreamFailureDetails = FfiStreamFailureDetails

/**
 * Structured barrier, publish-confirmation, or catch-up failure details.
 * A null target means target capture failed. Zero is a captured empty target.
 * Cursors and counts retain their full unsigned 64-bit values.
 */
val Throwable.streamFailureDetails: StreamFailureDetails?
    get() {
        val nativeError = if (this is XMTPException) cause else this
        if (nativeError !is FfiException) return null
        return nativeError.message?.let(::getStreamFailureDetails)
    }
