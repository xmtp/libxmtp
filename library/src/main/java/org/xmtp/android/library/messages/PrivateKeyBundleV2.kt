package org.xmtp.android.library.messages

import org.xmtp.android.library.XMTPException

typealias PrivateKeyBundleV2 = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKeyBundleV2

fun PrivateKeyBundleV2.sharedSecret(
    peer: SignedPublicKeyBundle,
    myPreKey: SignedPublicKey,
    isRecipient: Boolean,
): ByteArray {
    val dh1: ByteArray
    val dh2: ByteArray
    val preKey: SignedPrivateKey
    if (isRecipient) {
        preKey = findPreKey(myPreKey)
        dh1 = this.sharedSecret(
            preKey.secp256K1.bytes.toByteArray(),
            peer.identityKey.secp256K1Uncompressed.bytes.toByteArray()
        )
        dh2 = this.sharedSecret(
            identityKey.secp256K1.bytes.toByteArray(),
            peer.preKey.secp256K1Uncompressed.bytes.toByteArray()
        )
    } else {
        preKey = findPreKey(myPreKey)
        dh1 = this.sharedSecret(
            identityKey.secp256K1.bytes.toByteArray(),
            peer.preKey.secp256K1Uncompressed.bytes.toByteArray()
        )
        dh2 = this.sharedSecret(
            preKey.secp256K1.bytes.toByteArray(),
            peer.identityKey.secp256K1Uncompressed.bytes.toByteArray()
        )
    }
    val dh3 = this.sharedSecret(
        preKey.secp256K1.bytes.toByteArray(),
        peer.preKey.secp256K1Uncompressed.bytes.toByteArray()
    )
    return dh1 + dh2 + dh3
}

@OptIn(ExperimentalUnsignedTypes::class)
fun PrivateKeyBundleV2.sharedSecret(privateData: ByteArray, publicData: ByteArray): ByteArray {
    return uniffi.xmtpv3.diffieHellmanK256(privateData, publicData).toUByteArray().toByteArray()
}

fun PrivateKeyBundleV2.findPreKey(myPreKey: SignedPublicKey): SignedPrivateKey {
    for (preKey in preKeysList) {
        if (preKey.matches(myPreKey)) {
            return preKey
        }
    }
    throw XMTPException("No Pre key set")
}

fun PrivateKeyBundleV2.toV1(): PrivateKeyBundleV1 {
    return PrivateKeyBundleV1.newBuilder().also {
        it.identityKey = PrivateKeyBuilder.buildFromSignedPrivateKey(identityKey)
        it.addAllPreKeys(preKeysList.map { key -> PrivateKeyBuilder.buildFromSignedPrivateKey(key) })
    }.build()
}

fun PrivateKeyBundleV2.getPublicKeyBundle(): SignedPublicKeyBundle {
    return SignedPublicKeyBundle.newBuilder().also {
        it.identityKey = identityKey.publicKey
        it.identityKey = it.identityKey.toBuilder().also { idKeyBuilder ->
            idKeyBuilder.signature = identityKey.publicKey.signature.ensureWalletSignature()
        }.build()
        it.preKey = preKeysList[0].publicKey
    }.build()
}
