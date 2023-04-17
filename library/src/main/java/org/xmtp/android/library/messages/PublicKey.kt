package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.bouncycastle.util.Arrays
import org.web3j.crypto.Keys
import org.web3j.crypto.Sign
import org.xmtp.android.library.KeyUtil
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.PublicKeyOuterClass
import java.util.Date

typealias PublicKey = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKey

class PublicKeyBuilder {
    companion object {
        fun buildFromSignedPublicKey(signedPublicKey: PublicKeyOuterClass.SignedPublicKey): PublicKey {
            val unsignedPublicKey = PublicKey.parseFrom(signedPublicKey.keyBytes)
            return PublicKey.newBuilder().apply {
                timestamp = unsignedPublicKey.timestamp
                secp256K1Uncompressed = secp256K1Uncompressed.toBuilder().also {
                    it.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
                }.build()
                var sig = signedPublicKey.signature
                if (!sig.walletEcdsaCompact.bytes.isEmpty) {
                    sig = sig.toBuilder().apply {
                        ecdsaCompact = ecdsaCompact.toBuilder().also {
                            it.bytes = signedPublicKey.signature.walletEcdsaCompact.bytes
                            it.recovery = signedPublicKey.signature.walletEcdsaCompact.recovery
                        }.build()
                    }.build()
                }
                signature = sig
            }.build()
        }

        fun buildFromBytes(data: ByteArray): PublicKey {
            return PublicKey.newBuilder().apply {
                timestamp = Date().time
                secp256K1Uncompressed = secp256K1Uncompressed.toBuilder().apply {
                    bytes = data.toByteString()
                }.build()
            }.build()
        }
    }
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
        throw XMTPException("No signature found")
    }

    val slimKey = PublicKey.newBuilder().also {
        it.timestamp = timestamp
        it.secp256K1Uncompressed = it.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
            keyBuilder.bytes = secp256K1Uncompressed.bytes
        }.build()
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
