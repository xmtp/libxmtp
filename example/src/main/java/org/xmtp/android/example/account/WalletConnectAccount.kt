package org.xmtp.android.example.account

import dev.pinkroom.walletconnectkit.WalletConnectKit
import org.web3j.crypto.Keys
import org.web3j.utils.Numeric
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.messages.SignatureBuilder
import org.xmtp.proto.message.contents.SignatureOuterClass

data class WalletConnectAccount(private val wcKit: WalletConnectKit) : SigningKey {
    override val address: String
        get() = Keys.toChecksumAddress(wcKit.address.orEmpty())

    override suspend fun sign(data: ByteArray): SignatureOuterClass.Signature? {
        return sign(String(data))
    }

    override suspend fun sign(message: String): SignatureOuterClass.Signature? {
        runCatching { wcKit.personalSign(message) }
            .onSuccess {
                var result = it.result as String
                if (result.startsWith("0x") && result.length == 132) {
                    result = result.drop(2)
                }

                val resultData = Numeric.hexStringToByteArray(result)

                // Ensure we have a valid recovery byte
                resultData[resultData.size - 1] =
                    (1 - resultData[resultData.size - 1] % 2).toByte()

                return SignatureBuilder.buildFromSignatureData(resultData)
            }
            .onFailure {}
        return null
    }
}