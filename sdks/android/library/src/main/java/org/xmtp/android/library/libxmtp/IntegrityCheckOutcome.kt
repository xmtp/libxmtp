package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiIntegrityCheckOutcome

/**
 * Result of a [org.xmtp.android.library.Client.dbIntegrityCheck] /
 * [org.xmtp.android.library.Client.checkDatabaseIntegrity] run.
 *
 * [outcome] is one of `"ok"`, `"corrupt"`, `"unreadable"`, `"saltMissing"`,
 * `"locked"`, or `"failed"`. [findings] holds row-level findings for a
 * `"corrupt"` outcome, or the error/reason string for other non-`"ok"`
 * outcomes; it is empty when [outcome] is `"ok"`.
 */
data class IntegrityCheckOutcome(
    val outcome: String,
    val findings: List<String>,
) {
    internal constructor(ffi: FfiIntegrityCheckOutcome) : this(
        outcome = ffi.outcome,
        findings = ffi.findings,
    )
}
