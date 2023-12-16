package org.xmtp.android.library.messages

import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.PreEventCallback
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.XMTPException
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.security.SecureRandom

typealias PrivateKeyBundle = PrivateKeyOuterClass.PrivateKeyBundle

class PrivateKeyBundleBuilder {
    companion object {
        fun buildFromV1Key(v1: PrivateKeyBundleV1): PrivateKeyBundle {
            return PrivateKeyBundle.newBuilder().also {
                it.v1 = v1
            }.build()
        }
    }
}

fun PrivateKeyBundle.encrypted(
    key: SigningKey,
    preEnableIdentityCallback: PreEventCallback? = null,
): EncryptedPrivateKeyBundle {
    val bundleBytes = toByteArray()
    val walletPreKey = SecureRandom().generateSeed(32)

    preEnableIdentityCallback?.let {
        runBlocking {
            it.invoke()
        }
    }

    val signature =
        runBlocking {
            key.sign(
                message = Signature.newBuilder().build().enableIdentityText(key = walletPreKey)
            )
        } ?: throw XMTPException("Illegal signature")
    val cipherText = Crypto.encrypt(signature.rawDataWithNormalizedRecovery, bundleBytes)
    return EncryptedPrivateKeyBundle.newBuilder().apply {
        v1 = v1.toBuilder().also { v1Builder ->
            v1Builder.walletPreKey = walletPreKey.toByteString()
            v1Builder.ciphertext = cipherText
        }.build()
    }.build()
}
