package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.SigningKey
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.security.SecureRandom

typealias PrivateKeyBundle = PrivateKeyOuterClass.PrivateKeyBundle

fun PrivateKeyBundle.encrypted(key: SigningKey): EncryptedPrivateKeyBundle {
    val bundleBytes = toByteArray()
    val walletPreKey = SecureRandom().generateSeed(32)
    val signature =
        key.sign(message = Signature.newBuilder().build().enableIdentityText(key = walletPreKey))
    val cipherText = Crypto.encrypt(signature.rawData, bundleBytes)
    return EncryptedPrivateKeyBundle.newBuilder().apply {
        v1Builder.walletPreKey = walletPreKey.toByteString()
        v1Builder.ciphertext = cipherText
    }.build()
}
