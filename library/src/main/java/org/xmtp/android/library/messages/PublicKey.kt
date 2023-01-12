package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.crypto.digests.SHA256Digest
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.proto.message.contents.PublicKeyOuterClass

typealias PublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKey

class PublicKeyBuilder {
    companion object {
        fun buildFromSignedPublicKey(signedPublicKey: PublicKeyOuterClass.SignedPublicKey): PublicKey {
            val unsignedPublicKey = PublicKey.parseFrom(signedPublicKey.keyBytes)
            return unsignedPublicKey.toBuilder().apply {
                timestamp = unsignedPublicKey.timestamp
                secp256K1UncompressedBuilder.apply {
                    bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
                }.build()
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
        KeyUtil.getSignatureData(signature.toByteArray())
    )
    return PublicKeyBuilder.buildFromBytes(pubKeyData.toByteArray())
}

val PublicKeyOuterClass.PublicKey.walletAddress: String
    get() {
        return Keys.toChecksumAddress(Keys.getAddress(secp256K1Uncompressed.bytes.toString()))
    }