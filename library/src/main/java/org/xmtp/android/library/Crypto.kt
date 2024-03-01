package org.xmtp.android.library

import android.util.Log
import com.google.crypto.tink.subtle.Hkdf
import com.google.protobuf.kotlin.toByteString
import org.xmtp.proto.message.contents.CiphertextOuterClass
import java.security.GeneralSecurityException
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.Mac
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

        fun calculateMac(secret: ByteArray, message: ByteArray): ByteArray {
            val sha256HMAC: Mac = Mac.getInstance("HmacSHA256")
            val secretKey = SecretKeySpec(secret, "HmacSHA256")
            sha256HMAC.init(secretKey)
            return sha256HMAC.doFinal(message)
        }

        fun deriveKey(
            secret: ByteArray,
            salt: ByteArray,
            info: ByteArray,
        ): ByteArray {
            val keySpec = SecretKeySpec(secret, "HmacSHA256")
            val hmac = Mac.getInstance("HmacSHA256")
            hmac.init(keySpec)
            val derivedKey = hmac.doFinal(salt + info)

            return derivedKey.copyOfRange(0, 32)
        }

        fun verifyHmacSignature(
            key: ByteArray,
            signature: ByteArray,
            message: ByteArray
        ): Boolean {
            return try {
                val mac = Mac.getInstance("HmacSHA256")
                mac.init(SecretKeySpec(key, "HmacSHA256"))
                val computedSignature = mac.doFinal(message)
                computedSignature.contentEquals(signature)
            } catch (e: Exception) {
                false
            }
        }
    }
}
