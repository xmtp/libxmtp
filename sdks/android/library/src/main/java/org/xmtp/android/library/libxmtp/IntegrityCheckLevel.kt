package org.xmtp.android.library.libxmtp

import uniffi.xmtpv3.FfiIntegrityCheckLevel

/**
 * How thorough a [org.xmtp.android.library.Client.dbIntegrityCheck] /
 * [org.xmtp.android.library.Client.checkDatabaseIntegrity] run should be.
 */
enum class IntegrityCheckLevel {
    /**
     * Cheap structural checks without index-to-table cross-validation (and, on
     * encrypted databases, without per-page HMAC validation), safe to run
     * frequently.
     */
    QUICK,

    /**
     * Exhaustive check of every row and index entry. Slower; reserve for
     * diagnostics.
     */
    FULL,

    ;

    /**
     * Converts this Kotlin enum to FFI IntegrityCheckLevel
     */
    fun toFfi(): FfiIntegrityCheckLevel =
        when (this) {
            QUICK -> FfiIntegrityCheckLevel.QUICK
            FULL -> FfiIntegrityCheckLevel.FULL
        }
}
