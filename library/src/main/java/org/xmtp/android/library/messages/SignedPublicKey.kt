package org.xmtp.android.library.messages

import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.proto.message.contents.PublicKeyOuterClass

typealias SignedPublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.SignedPublicKey

class SignedPublicKeyBuilder {
    companion object {
        fun buildFromLegacy(
            legacyKey: PublicKey
        ): SignedPublicKey {
            val publicKey = PublicKey.newBuilder().apply {
                secp256K1Uncompressed = legacyKey.secp256K1Uncompressed
                timestamp = legacyKey.timestamp
            }.build()
            return SignedPublicKey.newBuilder().apply {
                keyBytes = publicKey.toByteString()
                signature = legacyKey.signature
            }.build()
        }

        fun parseFromPublicKey(publicKey: PublicKey, sig: Signature): SignedPublicKey {
            val builder = SignedPublicKey.newBuilder().apply {
                signature = sig
            }
            val unsignedKey = PublicKey.newBuilder().apply {
                timestamp = publicKey.timestamp
                secp256K1UncompressedBuilder.bytes = publicKey.secp256K1Uncompressed.bytes
            }.build()
            builder.keyBytes = unsignedKey.toByteString()
            return builder.build()
        }
    }
}

val SignedPublicKey.secp256K1Uncompressed: PublicKeyOuterClass.PublicKey.Secp256k1Uncompressed
    get() {
        val key = PublicKey.parseFrom(keyBytes)
        return key.secp256K1Uncompressed
    }

fun SignedPublicKey.verify(key: SignedPublicKey): Boolean {
    if (!key.hasSignature()) {
        return false
    }
    return signature.verify(
        PublicKeyBuilder.buildFromSignedPublicKey(key),
        key.keyBytes.toByteArray()
    )
}

fun SignedPublicKey.recoverKeySignedPublicKey(): PublicKey {
    val publicKey = PublicKeyBuilder.buildFromSignedPublicKey(this)
    val slimKey = PublicKey.newBuilder().also {
        it.secp256K1UncompressedBuilder.bytes = secp256K1Uncompressed.toByteString()
        it.timestamp = publicKey.timestamp
    }.build()

    val pubKeyData = Sign.signedMessageToKey(
        slimKey.toByteArray(),
        KeyUtil.getSignatureData(publicKey.signature.rawDataWithNormalizedRecovery)
    )
    return PublicKeyBuilder.buildFromBytes(pubKeyData.toByteArray())
}

fun SignedPublicKey.recoverWalletSignerPublicKey(): PublicKey {
    val publicKey = PublicKeyBuilder.buildFromSignedPublicKey(this)
    val sig = Signature.newBuilder().build()
    val sigText = sig.createIdentityText(keyBytes.toByteArray())
    val sigHash = sig.ethHash(sigText)
    val pubKeyData = Sign.signedMessageHashToKey(sigHash, KeyUtil.getSignatureData(publicKey.signature.rawDataWithNormalizedRecovery))
    return PublicKeyBuilder.buildFromBytes(KeyUtil.addUncompressedByte(pubKeyData.toByteArray()))
}
