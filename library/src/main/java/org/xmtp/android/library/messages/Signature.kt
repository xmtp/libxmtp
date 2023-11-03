package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Util
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.SignatureOuterClass

typealias Signature = org.xmtp.proto.message.contents.SignatureOuterClass.Signature

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

fun Signature.createIdentityText(key: ByteArray): String =
    ("XMTP : Create Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

fun Signature.enableIdentityText(key: ByteArray): String =
    ("XMTP : Enable Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

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

@OptIn(ExperimentalUnsignedTypes::class)
fun Signature.verify(signedBy: PublicKey, digest: ByteArray): Boolean {
    return try {
        uniffi.xmtp_dh.verifyK256Sha256(
            signedBy.secp256K1Uncompressed.bytes.toByteArray().toUByteArray().toList(),
            digest.toUByteArray().toList(),
            ecdsaCompact.bytes.toByteArray().toUByteArray().toList(),
            ecdsaCompact.recovery.toUByte()
        )
    } catch (e: Exception) {
        false
    }
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
