package org.xmtp.android.library.messages

import org.xmtp.proto.message.contents.PublicKeyOuterClass

typealias PublicKeyBundle = org.xmtp.proto.message.contents.PublicKeyOuterClass.PublicKeyBundle

class PublicKeyBundleBuilder {
    companion object {
        fun buildFromSignedKeyBundle(signedPublicKeyBundle: SignedPublicKeyBundle): PublicKeyBundle {
            return PublicKeyBundle.newBuilder().apply {
                identityKey = PublicKey.parseFrom(signedPublicKeyBundle.identityKey.keyBytes)
                preKey = PublicKey.parseFrom(signedPublicKeyBundle.preKey.keyBytes)
            }.build()
        }
    }
}

val PublicKeyBundle.walletAddress: String
    get() =
        (try { identityKey.recoverWalletSignerPublicKey().walletAddress } catch (e: Throwable) { null }) ?: ""
