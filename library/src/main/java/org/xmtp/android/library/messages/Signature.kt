package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.jce.ECNamedCurveTable
import org.bouncycastle.jce.ECPointUtil
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.jce.spec.ECNamedCurveParameterSpec
import org.bouncycastle.jce.spec.ECNamedCurveSpec
import org.bouncycastle.util.Arrays
import org.xmtp.android.library.Util
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.SignatureOuterClass
import java.math.BigInteger
import java.security.KeyFactory
import java.security.interfaces.ECPublicKey
import java.security.spec.ECPublicKeySpec

typealias Signature = org.xmtp.proto.message.contents.SignatureOuterClass.Signature

private const val MESSAGE_PREFIX = "\u0019Ethereum Signed Message:\n"

class SignatureBuilder {
    companion object {
        fun buildFromSignatureData(data: ByteArray): Signature {
            return Signature.newBuilder().also {
                it.ecdsaCompactBuilder.bytes = data.take(64).toByteArray().toByteString()
                it.ecdsaCompactBuilder.recovery = data[64].toInt()
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
    val ecdsaVerify = java.security.Signature.getInstance("SHA256withECDSA", BouncyCastleProvider())
    ecdsaVerify.initVerify(getPublicKeyFromBytes(signedBy.secp256K1Uncompressed.bytes.toByteArray()))
    ecdsaVerify.update(digest)
    return ecdsaVerify.verify(normalizeSignatureForVerification(this.rawDataWithNormalizedRecovery))
}

private fun normalizeSignatureForVerification(signature: ByteArray): ByteArray {
    val r: ByteArray = BigInteger(1, Arrays.copyOfRange(signature, 0, 32)).toByteArray()
    val s: ByteArray = BigInteger(1, Arrays.copyOfRange(signature, 32, 64)).toByteArray()
    val der = ByteArray(6 + r.size + s.size)
    der[0] = 0x30 // Tag of signature object

    der[1] = (der.size - 2).toByte() // Length of signature object

    var o = 2
    der[o++] = 0x02 // Tag of ASN1 Integer

    der[o++] = r.size.toByte() // Length of first signature part

    System.arraycopy(r, 0, der, o, r.size)
    o += r.size
    der[o++] = 0x02 // Tag of ASN1 Integer

    der[o++] = s.size.toByte() // Length of second signature part

    System.arraycopy(s, 0, der, o, s.size)

    return der
}

private fun getPublicKeyFromBytes(pubKey: ByteArray): java.security.PublicKey {
    val spec: ECNamedCurveParameterSpec = ECNamedCurveTable.getParameterSpec("secp256k1")
    val kf: KeyFactory = KeyFactory.getInstance("ECDSA", BouncyCastleProvider())
    val params = ECNamedCurveSpec("secp256k1", spec.curve, spec.g, spec.n)
    val point = ECPointUtil.decodePoint(params.curve, pubKey)
    val pubKeySpec = ECPublicKeySpec(point, params)
    return kf.generatePublic(pubKeySpec) as ECPublicKey
}

fun Signature.ensureWalletSignature(): Signature {
    when (unionCase) {
        SignatureOuterClass.Signature.UnionCase.ECDSA_COMPACT -> {
            val walletEcdsa = SignatureOuterClass.Signature.WalletECDSACompact.newBuilder().also {
                it.bytes = ecdsaCompact.bytes
                it.recovery = ecdsaCompact.recovery
            }.build()
            return this.toBuilder().also {
                it.walletEcdsaCompact = walletEcdsa
            }.build()
        }
        else -> return this
    }
}
