package org.xmtp.android.library

import uniffi.xmtpv3.FfiApiStats
import uniffi.xmtpv3.FfiIdentityStats
import uniffi.xmtpv3.FfiXmtpClient

class XMTPDebugInformation(
    private val ffiClient: FfiXmtpClient,
    private val client: Client,
) {
    val apiStatistics: ApiStats
        get() = ApiStats(ffiClient.apiStatistics())
    val identityStatistics: IdentityStats
        get() = IdentityStats(ffiClient.apiIdentityStatistics())
    val aggregateStatistics: String
        get() = ffiClient.apiAggregateStatistics()

    suspend fun uploadDebugInformation(serverUrl: String = client.environment.getHistorySyncUrl()): String {
        return ffiClient.uploadDebugArchive(serverUrl)
    }

    fun clearAllStatistics() {
        return ffiClient.clearAllStatistics()
    }
}

class ApiStats(
    private val apiStats: FfiApiStats,
) {
    val uploadKeyPackage: Long
        get() = apiStats.uploadKeyPackage.toLong()
    val fetchKeyPackage: Long
        get() = apiStats.fetchKeyPackage.toLong()
    val sendGroupMessages: Long
        get() = apiStats.sendGroupMessages.toLong()
    val sendWelcomeMessages: Long
        get() = apiStats.sendWelcomeMessages.toLong()
    val queryGroupMessages: Long
        get() = apiStats.queryGroupMessages.toLong()
    val queryWelcomeMessages: Long
        get() = apiStats.queryWelcomeMessages.toLong()
    val subscribeMessages: Long
        get() = apiStats.subscribeMessages.toLong()
    val subscribeWelcomes: Long
        get() = apiStats.subscribeWelcomes.toLong()
}

class IdentityStats(
    private val identityStats: FfiIdentityStats,
) {
    val publishIdentityUpdate: Long
        get() = identityStats.publishIdentityUpdate.toLong()
    val getIdentityUpdatesV2: Long
        get() = identityStats.getIdentityUpdatesV2.toLong()
    val getInboxIds: Long
        get() = identityStats.getInboxIds.toLong()
    val verifySmartContractWalletSignature: Long
        get() = identityStats.verifySmartContractWalletSignature.toLong()
}
