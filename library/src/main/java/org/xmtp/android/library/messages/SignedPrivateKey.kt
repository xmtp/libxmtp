package org.xmtp.android.library.messages

import kotlinx.coroutines.runBlocking

typealias SignedPrivateKey = org.xmtp.proto.message.contents.PrivateKeyOuterClass.SignedPrivateKey

class SignedPrivateKeyBuilder {
    companion object {
        fun buildFromLegacy(key: PrivateKey): SignedPrivateKey {
            return SignedPrivateKey.newBuilder().apply {
                createdNs = key.timestamp * 1_000_000
                secp256K1 = secp256K1.toBuilder().also {
                    it.bytes = key.secp256K1.bytes
                }.build()
                publicKey = SignedPublicKeyBuilder.buildFromLegacy(key.publicKey)
                publicKey = publicKey.toBuilder().also {
                    it.signature = key.publicKey.signature
                }.build()
            }.build()
        }
    }
}

fun SignedPrivateKey.sign(data: ByteArray): Signature {
    val key = PrivateKeyBuilder.buildFromPrivateKeyData(secp256K1.bytes.toByteArray())
    return runBlocking {
        PrivateKeyBuilder(key).sign(data)
    }
}

fun SignedPrivateKey.matches(signedPublicKey: SignedPublicKey): Boolean {
    return publicKey.recoverWalletSignerPublicKey().walletAddress == signedPublicKey.recoverWalletSignerPublicKey().walletAddress
}
