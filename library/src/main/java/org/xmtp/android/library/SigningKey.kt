package org.xmtp.android.library

import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.proto.message.contents.SignatureOuterClass

interface SigningKey {
    val publicIdentity: PublicIdentity

    // The wallet type if Smart Contract Wallet this should be type SCW.
    val type: SignerType
        get() = SignerType.EOA

    // The chainId of the Smart Contract Wallet value should be null if not SCW
    var chainId: Long?
        get() = null
        set(_) {}

    // Default blockNumber value set to null
    var blockNumber: Long?
        get() = null
        set(_) {}

    suspend fun sign(data: ByteArray): SignatureOuterClass.Signature? {
        throw NotImplementedError("sign(ByteArray) is not implemented.")
    }

    suspend fun sign(message: String): SignatureOuterClass.Signature? {
        throw NotImplementedError("sign(String) is not implemented.")
    }

    suspend fun signSCW(message: String): ByteArray {
        throw NotImplementedError("signSCW(String) is not implemented.")
    }
}

enum class SignerType {
    SCW, // Smart Contract Wallet
    EOA, // Externally Owned Account *Default
    PASSKEY
}
