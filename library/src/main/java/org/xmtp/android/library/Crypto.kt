package org.xmtp.android.library

import android.util.Log
import com.google.crypto.tink.subtle.Hkdf
import com.google.protobuf.kotlin.toByteString
import org.xmtp.proto.message.contents.CiphertextOuterClass
import java.security.GeneralSecurityException
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

typealias CipherText = CiphertextOuterClass.Ciphertext

class Crypto {
    companion object {
        private const val TAG = "Crypto"
        fun encrypt(
            secret: ByteArray,
            message: ByteArray,
            additionalData: ByteArray = byteArrayOf(),
        ): CipherText? {
            return try {
                val salt = SecureRandom().generateSeed(32)
                val nonceData = SecureRandom().generateSeed(12)
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")

                val key = Hkdf.computeHkdf("HMACSHA256", secret, salt, null, 32)
                val keySpec = SecretKeySpec(key, "AES")
                val gcmSpec = GCMParameterSpec(128, nonceData)

                cipher.init(Cipher.ENCRYPT_MODE, keySpec, gcmSpec)
                if (additionalData.isNotEmpty()) {
                    cipher.updateAAD(additionalData)
                }
                val final = cipher.doFinal(message)

                CiphertextOuterClass.Ciphertext.newBuilder().apply {
                    aes256GcmHkdfSha256 = aes256GcmHkdfSha256.toBuilder().also {
                        it.payload = final.toByteString()
                        it.hkdfSalt = salt.toByteString()
                        it.gcmNonce = nonceData.toByteString()
                    }.build()
                }.build()
            } catch (err: GeneralSecurityException) {
                Log.e(TAG, err.message.toString())
                null
            }
        }

        fun decrypt(
            secret: ByteArray,
            ciphertext: CipherText,
            additionalData: ByteArray = byteArrayOf(),
        ): ByteArray? {
            return try {
                val salt = ciphertext.aes256GcmHkdfSha256.hkdfSalt.toByteArray()
                val nonceData = ciphertext.aes256GcmHkdfSha256.gcmNonce.toByteArray()
                val payload = ciphertext.aes256GcmHkdfSha256.payload.toByteArray()
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")

                val key = Hkdf.computeHkdf("HMACSHA256", secret, salt, null, 32)
                val keySpec = SecretKeySpec(key, "AES")
                val gcmSpec = GCMParameterSpec(128, nonceData)

                cipher.init(Cipher.DECRYPT_MODE, keySpec, gcmSpec)
                if (additionalData.isNotEmpty()) {
                    cipher.updateAAD(additionalData)
                }
                cipher.doFinal(payload)
            } catch (err: GeneralSecurityException) {
                Log.e(TAG, err.message.toString())
                null
            }
        }
    }
}
