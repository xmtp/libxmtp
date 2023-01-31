package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.sharedSecret
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.proto.message.contents.PrivateKeyOuterClass

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

    @Test
    fun testMessages() {
        val aliceWallet = PrivateKeyBuilder()
        val bobWallet = PrivateKeyBuilder()
        val alice = PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val msg = "Hello world"
        val decrypted = msg.toByteStringUtf8().toByteArray()
        val alicePublic = alice.toPublicKeyBundle()
        val bobPublic = bob.toPublicKeyBundle()
        val aliceSecret = alice.sharedSecret(peer = bobPublic, myPreKey = alicePublic.preKey, isRecipient = false)
        val encrypted = Crypto.encrypt(aliceSecret, decrypted)
        val bobSecret = bob.sharedSecret(peer = alicePublic, myPreKey = bobPublic.preKey, isRecipient = true)
        val bobDecrypted = Crypto.decrypt(bobSecret, encrypted!!)
        val decryptedText = String(bobDecrypted!!, Charsets.UTF_8)
        assertEquals(decryptedText, msg)
    }
}
