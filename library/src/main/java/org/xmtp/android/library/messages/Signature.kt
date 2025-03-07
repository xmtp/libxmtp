package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Util
import org.xmtp.proto.message.contents.SignatureOuterClass

typealias Signature = SignatureOuterClass.Signature

private const val MESSAGE_PREFIX = "\u0019Ethereum Signed Message:\n"

class SignatureBuilder {
    companion object {
        fun buildFromSignatureData(data: ByteArray): Signature {
            return Signature.newBuilder().also {
                it.ecdsaCompact = it.ecdsaCompact.toBuilder().also { builder ->
                    builder.bytes = data.take(64).toByteArray().toByteString()
                    builder.recovery = data[64].toInt()
                }.build()
            }.build()
        }
    }
}

fun Signature.ethHash(message: String): ByteArray {
    val input = MESSAGE_PREFIX + message.length + message
    return Util.keccak256(input.toByteArray())
}

val Signature.rawData: ByteArray
    get() = if (hasEcdsaCompact()) {
        ecdsaCompact.bytes.toByteArray() + ecdsaCompact.recovery.toByte()
    } else {
        walletEcdsaCompact.bytes.toByteArray() + walletEcdsaCompact.recovery.toByte()
    }

val Signature.rawDataWithNormalizedRecovery: ByteArray
    get() {
        val data = rawData
        if (data[64] == 0.toByte()) {
            data[64] = 27.toByte()
        } else if (data[64] == 1.toByte()) {
            data[64] = 28.toByte()
        }
        return data
    }

fun Signature.ensureWalletSignature(): Signature {
    return when (unionCase) {
        SignatureOuterClass.Signature.UnionCase.ECDSA_COMPACT -> {
            val walletEcdsa = SignatureOuterClass.Signature.WalletECDSACompact.newBuilder().also {
                it.bytes = ecdsaCompact.bytes
                it.recovery = ecdsaCompact.recovery
            }.build()
            this.toBuilder().also {
                it.walletEcdsaCompact = walletEcdsa
            }.build()
        }

        else -> this
    }
}
