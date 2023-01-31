package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.crypto.digests.SHA256Digest
import org.bouncycastle.util.Arrays
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.PublicKeyOuterClass

typealias PublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKey

class PublicKeyBuilder {
    companion object {
        fun buildFromSignedPublicKey(signedPublicKey: PublicKeyOuterClass.SignedPublicKey): PublicKey {
            val unsignedPublicKey = PublicKey.parseFrom(signedPublicKey.keyBytes)
            return PublicKey.newBuilder().apply {
                timestamp = unsignedPublicKey.timestamp
                secp256K1UncompressedBuilder.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes

                var sig = signedPublicKey.signature
                if (!sig.walletEcdsaCompact.bytes.isEmpty) {
                    sig = sig.toBuilder().apply {
                        ecdsaCompactBuilder.bytes =
                            signedPublicKey.signature.walletEcdsaCompact.bytes
                        ecdsaCompactBuilder.recovery =
                            signedPublicKey.signature.walletEcdsaCompact.recovery
                    }.build()
                }
                signature = sig
            }.build()
        }

        fun buildFromBytes(data: ByteArray): PublicKey {
            return PublicKey.newBuilder().apply {
                timestamp = System.currentTimeMillis()
                secp256K1UncompressedBuilder.apply {
                    bytes = data.toByteString()
                }.build()
            }.build()
        }
    }
}

fun PublicKey.recoverKeySignedPublicKey(): PublicKey {
    if (!hasSignature()) {
        throw IllegalArgumentException("No signature found")
    }
    val bytesToSign = PublicKey.newBuilder().apply {
        secp256K1UncompressedBuilder.apply {
            bytes = secp256K1Uncompressed.bytes
        }.build()
        this.timestamp = timestamp
    }.build().toByteArray()

    val pubKeyData = Sign.signedMessageToKey(
        SHA256Digest(bytesToSign).encodedState,
        KeyUtil.getSignatureData(signature.toByteArray()),
    )
    return PublicKeyBuilder.buildFromBytes(pubKeyData.toByteArray())
}

val PublicKey.walletAddress: String
    get() {
        val address = Keys.getAddress(
            Arrays.copyOfRange(
                secp256K1Uncompressed.bytes.toByteArray(),
                1,
                secp256K1Uncompressed.bytes.toByteArray().size
            )
        )
        return Keys.toChecksumAddress(address.toHex())
    }

fun PublicKey.recoverWalletSignerPublicKey(): PublicKey {
    if (!hasSignature()) {
        throw IllegalArgumentException("No signature found")
    }
    val slimKey = PublicKey.newBuilder().also {
        it.timestamp = timestamp
        it.secp256K1UncompressedBuilder.bytes = secp256K1Uncompressed.bytes
    }.build()
    val signatureClass = Signature.newBuilder().build()
    val sigText = signatureClass.createIdentityText(slimKey.toByteArray())
    val sigHash = signatureClass.ethHash(sigText)
    val pubKeyData = Sign.signedMessageHashToKey(
        sigHash,
        KeyUtil.getSignatureData(signature.rawDataWithNormalizedRecovery)
    )
    return PublicKeyBuilder.buildFromBytes(KeyUtil.addUncompressedByte(pubKeyData.toByteArray()))
}
