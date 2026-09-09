package org.xmtp.android.library

import uniffi.xmtpv3.FfiApiStats
import uniffi.xmtpv3.FfiIdentityStats
import uniffi.xmtpv3.FfiXmtpClient

class XMTPDebugInformation(
    private val ffiClient: FfiXmtpClient,
) {
    val apiStatistics: ApiStats
        get() = ApiStats(ffiClient.apiStatistics())
    val identityStatistics: IdentityStats
        get() = IdentityStats(ffiClient.apiIdentityStatistics())
    val aggregateStatistics: String
        get() = ffiClient.apiAggregateStatistics()

    fun clearAllStatistics() = ffiClient.clearAllStatistics()
}

class ApiStats(
    private val apiStats: FfiApiStats,
) {
    val publish: Long
        get() = apiStats.publish.toLong()
    val query: Long
        get() = apiStats.query.toLong()
    val queryNewest: Long
        get() = apiStats.queryNewest.toLong()
    val get: Long
        get() = apiStats.get.toLong()
    val subscribe: Long
        get() = apiStats.subscribe.toLong()
    val subscribeStatic: Long
        get() = apiStats.subscribeStatic.toLong()
}

class IdentityStats(
    private val identityStats: FfiIdentityStats,
) {
    val getInboxIds: Long
        get() = identityStats.getInboxIds.toLong()
    val verifySmartContractWalletSignatures: Long
        get() = identityStats.verifySmartContractWalletSignatures.toLong()
}
