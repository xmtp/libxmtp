package org.xmtp.android.library.messages

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.PreEventCallback
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.XMTPException

typealias EncryptedPrivateKeyBundle = org.xmtp.proto.message.contents.PrivateKeyOuterClass.EncryptedPrivateKeyBundle

fun EncryptedPrivateKeyBundle.decrypted(
    key: SigningKey,
    preEnableIdentityCallback: PreEventCallback? = null,
): PrivateKeyBundle {
    preEnableIdentityCallback?.let {
        runBlocking {
            it.invoke()
        }
    }

    val signature = runBlocking {
        key.signLegacy(
            message = Signature.newBuilder().build()
                .enableIdentityText(key = v1.walletPreKey.toByteArray()),
        )
    } ?: throw XMTPException("Illegal signature")
    val message = Crypto.decrypt(signature.rawDataWithNormalizedRecovery, v1.ciphertext)
    return PrivateKeyBundle.parseFrom(message)
}
