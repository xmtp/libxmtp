package org.xmtp.android.library

import uniffi.xmtpv3.FfiStreamSettings

/**
 * Optional stream limits. Null fields use native defaults. All timer values are milliseconds.
 * Native client creation validates the supplied values.
 */
data class StreamSettings(
    val maxAdmissionRows: UInt? = null,
    val maxAdmissionBytes: ULong? = null,
    val maxFetchedRows: UInt? = null,
    val maxFetchedBytes: ULong? = null,
    val groupPendingRows: ULong? = null,
    val groupPendingBytes: ULong? = null,
    val welcomePendingRows: ULong? = null,
    val welcomePendingBytes: ULong? = null,
    val identityPendingRows: ULong? = null,
    val identityPendingBytes: ULong? = null,
    val maxPendingRowsPerTopic: ULong? = null,
    val maxPendingBytesPerTopic: ULong? = null,
    val maxDependencyRequests: UInt? = null,
    val maxLocalReadRows: UInt? = null,
    val maxLocalReadBytes: ULong? = null,
    val receiverFallbackIntervalMs: ULong? = null,
    val activeDatabasePollIntervalMs: ULong? = null,
    val defaultConsumerLeaseDurationMs: ULong? = null,
    val identityReferenceWaitMs: ULong? = null,
    val barrierTimeoutMs: ULong? = null,
) {
    internal fun toFfi(): FfiStreamSettings =
        FfiStreamSettings(
            maxAdmissionRows = maxAdmissionRows,
            maxAdmissionBytes = maxAdmissionBytes,
            maxFetchedRows = maxFetchedRows,
            maxFetchedBytes = maxFetchedBytes,
            groupPendingRows = groupPendingRows,
            groupPendingBytes = groupPendingBytes,
            welcomePendingRows = welcomePendingRows,
            welcomePendingBytes = welcomePendingBytes,
            identityPendingRows = identityPendingRows,
            identityPendingBytes = identityPendingBytes,
            maxPendingRowsPerTopic = maxPendingRowsPerTopic,
            maxPendingBytesPerTopic = maxPendingBytesPerTopic,
            maxDependencyRequests = maxDependencyRequests,
            maxLocalReadRows = maxLocalReadRows,
            maxLocalReadBytes = maxLocalReadBytes,
            receiverFallbackIntervalMs = receiverFallbackIntervalMs,
            activeDatabasePollIntervalMs = activeDatabasePollIntervalMs,
            defaultConsumerLeaseDurationMs = defaultConsumerLeaseDurationMs,
            identityReferenceWaitMs = identityReferenceWaitMs,
            barrierTimeoutMs = barrierTimeoutMs,
        )
}
