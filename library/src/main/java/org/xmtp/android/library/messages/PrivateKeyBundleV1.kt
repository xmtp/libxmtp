package org.xmtp.android.library.messages

import org.web3j.crypto.Hash
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.createIdentity

typealias PrivateKeyBundleV1 = org.xmtp.proto.message.contents.PrivateKeyOuterClass.PrivateKeyBundleV1

fun PrivateKeyBundleV1.generate(wallet: SigningKey): PrivateKeyBundleV1 {
    val privateKey = PrivateKeyBuilder()
    val authorizedIdentity = wallet.createIdentity(privateKey.getPrivateKey())
    var bundle = authorizedIdentity.toBundle
    var preKey = PrivateKey.newBuilder().build().generate()
    val bytesToSign = UnsignedPublicKeyBuilder.buildFromPublicKey(preKey.publicKey).toByteArray()
    val signature = privateKey.sign(Hash.sha256(bytesToSign))

    preKey = preKey.toBuilder().apply {
        publicKeyBuilder.signature = signature
    }.build()

    val signedPublicKey = privateKey.getPrivateKey()
        .sign(key = UnsignedPublicKeyBuilder.buildFromPublicKey(preKey.publicKey))

    preKey = preKey.toBuilder().apply {
        publicKey = PublicKeyBuilder.buildFromSignedPublicKey(signedPublicKey)
        publicKeyBuilder.signature = signedPublicKey.signature
    }.build()

    bundle = bundle.toBuilder().apply {
        v1Builder.apply {
            identityKey = authorizedIdentity.identity
            identityKeyBuilder.publicKey = authorizedIdentity.authorized
            addPreKeys(preKey)
        }.build()
    }.build()

    return bundle.v1
}

val PrivateKeyBundleV1.walletAddress: String
    get() = identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress

fun PrivateKeyBundleV1.toV2(): PrivateKeyBundleV2 {
    return PrivateKeyBundleV2.newBuilder().also {
        it.identityKey =
            SignedPrivateKeyBuilder.buildFromLegacy(identityKey)
        it.addAllPreKeys(preKeysList.map { key -> SignedPrivateKeyBuilder.buildFromLegacy(key) })
    }.build()
}

fun PrivateKeyBundleV1.toPublicKeyBundle(): PublicKeyBundle {
    return PublicKeyBundle.newBuilder().also {
        it.identityKey = identityKey.publicKey
        it.preKey = preKeysList[0].publicKey
    }.build()
}

fun PrivateKeyBundleV1.sharedSecret(
    peer: PublicKeyBundle,
    myPreKey: PublicKey,
    isRecipient: Boolean
): ByteArray {
    val peerBundle = SignedPublicKeyBundleBuilder.buildFromKeyBundle(peer)
    val preKey = SignedPublicKeyBuilder.buildFromLegacy(myPreKey)
    return toV2().sharedSecret(peer = peerBundle, myPreKey = preKey, isRecipient = isRecipient)
}
