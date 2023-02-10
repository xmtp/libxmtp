package org.xmtp.android.library

import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.walletAddress

class TestHelpers {
    class FakeWallet : SigningKey {
        private var privateKey: PrivateKey
        private var privateKeyBuilder: PrivateKeyBuilder

        constructor(key: PrivateKey, builder: PrivateKeyBuilder) {
            privateKey = key
            privateKeyBuilder = builder
        }

        companion object {
            fun generate(): FakeWallet {
                val key = PrivateKeyBuilder()
                return FakeWallet(key.getPrivateKey(), key)
            }
        }

        override val address: String
            get() = privateKey.walletAddress

        override fun sign(data: ByteArray): Signature {
            val signature = privateKeyBuilder.sign(data)
            return signature
        }

        override fun sign(message: String): Signature {
            val signature = privateKeyBuilder.sign(message)
            return signature
        }
    }
}
