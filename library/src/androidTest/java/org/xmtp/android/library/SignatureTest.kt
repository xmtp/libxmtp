package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.runner.RunWith
import org.web3j.crypto.Hash
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.consentProofText
import org.xmtp.android.library.messages.verify

@RunWith(AndroidJUnit4::class)
class SignatureTest {
    @Test
    fun testVerify() {
        val digest = Hash.sha256("Hello world".toByteStringUtf8().toByteArray())
        val signingKey = PrivateKeyBuilder()
        val signature = runBlocking { signingKey.sign(digest) }
        assert(
            signature.verify(
                signedBy = signingKey.getPrivateKey().publicKey,
                digest = "Hello world".toByteStringUtf8().toByteArray()
            )
        )
    }

    @Test
    fun testConsentProofText() {
        val timestamp = 1581663600000
        val exampleAddress = "0x1234567890abcdef"
        val signatureClass = Signature.newBuilder().build()
        val text = signatureClass.consentProofText(exampleAddress, timestamp)
        val expected = "XMTP : Grant inbox consent to sender\n\nCurrent Time: Fri, 14 Feb 2020 07:00:00 GMT\nFrom Address: 0x1234567890abcdef\n\nFor more info: https://xmtp.org/signatures/"
        assert(text == expected)
    }
}
