package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.runner.RunWith
import org.web3j.crypto.Hash
import org.xmtp.android.library.messages.PrivateKeyBuilder
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
}
