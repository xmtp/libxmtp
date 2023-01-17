package org.xmtp.android.library.messages

import org.xmtp.android.library.Crypto
import org.xmtp.android.library.SigningKey

typealias EncryptedPrivateKeyBundle = org.xmtp.proto.message.contents.PrivateKeyOuterClass.EncryptedPrivateKeyBundle

fun EncryptedPrivateKeyBundle.decrypted(key: SigningKey): PrivateKeyBundle {
    val signature = key.sign(
        message = Signature.newBuilder().build()
            .enableIdentityText(key = v1.walletPreKey.toByteArray()),
    )
    val message = Crypto.decrypt(signature.rawData, v1.ciphertext)
    return PrivateKeyBundle.parseFrom(message)
}
