package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import org.junit.Assert.assertEquals
import org.junit.Test

class CryptoTest {

    @Test
    fun testCodec() {
        val message = byteArrayOf(5, 5, 5)
        val secret = byteArrayOf(1, 2, 3, 4)
        val encrypted = Crypto.encrypt(secret, message)
        val decrypted = Crypto.decrypt(secret, encrypted!!)
        assertEquals(message.toByteString(), decrypted!!.toByteString())
    }

    @Test
    fun testDecryptingKnownCypherText() {
        val message = byteArrayOf(5, 5, 5)
        val secret = byteArrayOf(1, 2, 3, 4)
        val ints = arrayOf( // This was generated using xmtp-js code for encrypt().
            10, 69, 10, 32, 23, 10, 217, 190, 235, 216, 145,
            38, 49, 224, 165, 169, 22, 55, 152, 150, 176, 65,
            207, 91, 45, 45, 16, 171, 146, 125, 143, 60, 152, 128,
            0, 120, 18, 12, 219, 247, 207, 184, 141, 179, 171, 100,
            251, 171, 120, 137, 26, 19, 216, 215, 152, 167, 118, 59,
            93, 177, 53, 242, 147, 10, 87, 143, 27, 245, 154, 169, 109,
        )
        val bytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }
        val encrypted = CipherText.parseFrom(bytes)
        val decrypted = Crypto.decrypt(secret, encrypted)
        assertEquals(message.toByteString(), decrypted!!.toByteString())
    }
}
