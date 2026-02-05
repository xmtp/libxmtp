package org.xmtp.android.library

import org.xmtp.android.library.libxmtp.PublicIdentity

interface SigningKey {
    val publicIdentity: PublicIdentity

    val type: SignerType
        get() = SignerType.EOA

    // The chainId of the Smart Contract Wallet
    var chainId: Long?
        get() = null
        set(_) {}

    // Default blockNumber value set to null
    var blockNumber: Long?
        get() = null
        set(_) {}

    suspend fun sign(message: String): SignedData
}

enum class SignerType {
    SCW, // Smart Contract Wallet
    EOA, // Externally Owned Account *Default
}
