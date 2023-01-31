package org.xmtp.android.library.messages

typealias SignedPrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.SignedPrivateKey

class SignedPrivateKeyBuilder {
    companion object {
        fun buildFromLegacy(key: PrivateKey): SignedPrivateKey {
            return SignedPrivateKey.newBuilder().apply {
                createdNs = key.timestamp * 1_000_000
                secp256K1Builder.bytes = key.secp256K1.bytes
                publicKey = SignedPublicKeyBuilder.buildFromLegacy(
                    key.publicKey,
                )
                publicKeyBuilder.signature = key.publicKey.signature
            }.build()
        }
    }
}

fun SignedPrivateKey.sign(data: ByteArray): Signature {
    val key = PrivateKey.parseFrom(secp256K1.bytes)
    return PrivateKeyBuilder(key).sign(data)
}

fun SignedPrivateKey.matches(signedPublicKey: SignedPublicKey): Boolean =
    publicKey == signedPublicKey
