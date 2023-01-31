package org.xmtp.android.library.messages

import org.web3j.crypto.ECDSASignature
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.Util
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.math.BigInteger

typealias Signature = org.xmtp.proto.message.contents.SignatureOuterClass.Signature

private const val MESSAGE_PREFIX = "\u0019Ethereum Signed Message:\n"

fun Signature.ethHash(message: String): ByteArray {
    val input = MESSAGE_PREFIX + message.length + message
    return Util.keccak256(input.toByteArray())
}

fun Signature.createIdentityText(key: ByteArray): String =
    ("XMTP : Create Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

fun Signature.enableIdentityText(key: ByteArray): String =
    ("XMTP : Enable Identity\n" + "${key.toHex()}\n" + "\n" + "For more info: https://xmtp.org/signatures/")

val Signature.rawData: ByteArray
    get() = ecdsaCompact.bytes.toByteArray() + ecdsaCompact.recovery.toByte()

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

fun Signature.verify(signedBy: PublicKey, digest: ByteArray): Boolean {
    val signatureData = KeyUtil.getSignatureData(ecdsaCompact.bytes.toByteArray() + ecdsaCompact.recovery.toByte())
    val publicKey = Sign.recoverFromSignature(
        BigInteger(1, signatureData.v).toInt(),
        ECDSASignature(BigInteger(1, signatureData.r), BigInteger(1, signatureData.s)),
        digest,
    )
    val pubKey = KeyUtil.addUncompressedByte(publicKey.toByteArray())
    return pubKey.contentEquals(signedBy.secp256K1Uncompressed.bytes.toByteArray())
}

fun Signature.ensureWalletSignature() {
    when (unionCase) {
        SignatureOuterClass.Signature.UnionCase.ECDSA_COMPACT -> {
            val walletEcdsa = SignatureOuterClass.Signature.WalletECDSACompact.newBuilder().also {
                it.bytes = ecdsaCompact.bytes
                it.recovery = ecdsaCompact.recovery
            }.build()
            this.toBuilder().apply {
                walletEcdsaCompact = walletEcdsa
            }.build()
        }
        else -> return
    }
}
