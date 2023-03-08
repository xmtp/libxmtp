package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteStringUtf8
import org.junit.Test
import org.web3j.crypto.Hash
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.verify

class SignatureTest {
    @Test
    fun testVerify() {
        val digest = Hash.sha256("Hello world".toByteStringUtf8().toByteArray())
        val signingKey = PrivateKeyBuilder()
        val signature = signingKey.sign(digest)
        assert(
            signature.verify(
                signedBy = signingKey.getPrivateKey().publicKey,
                digest = "Hello world".toByteStringUtf8().toByteArray()
            )
        )
    }
}
