package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.SigningKey
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.security.SecureRandom

typealias PrivateKeyBundle = PrivateKeyOuterClass.PrivateKeyBundle

class PrivateKeyBundleBuilder {
    companion object {
        fun buildFromV1Key(v1: PrivateKeyBundleV1): PrivateKeyBundle {
            return PrivateKeyBundle.newBuilder().apply {
                this.v1 = v1
            }.build()
        }
    }
}

fun PrivateKeyBundle.encrypted(key: SigningKey): EncryptedPrivateKeyBundle {
    val bundleBytes = toByteArray()
    val walletPreKey = SecureRandom().generateSeed(32)
    val signature =
        key.sign(message = Signature.newBuilder().build().enableIdentityText(key = walletPreKey))
    val cipherText = Crypto.encrypt(signature.rawDataWithNormalizedRecovery, bundleBytes)
    return EncryptedPrivateKeyBundle.newBuilder().apply {
        v1Builder.walletPreKey = walletPreKey.toByteString()
        v1Builder.ciphertext = cipherText
    }.build()
}
