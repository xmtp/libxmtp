package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder

class ClientTest {
    @Test
    fun testTakesAWallet() {
        val fakeWallet = PrivateKeyBuilder()
        Client().create(account = fakeWallet)
    }

    @Test
    fun testHasPrivateKeyBundleV1() {
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet)
        assertEquals(1, client.privateKeyBundleV1?.preKeysList?.size)
        val preKey = client.privateKeyBundleV1?.preKeysList?.get(0)
        assert(preKey?.publicKey?.hasSignature() ?: false)
    }

    @Test
    fun testCanBeCreatedWithBundle() {
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet)
        val bundle = client.privateKeyBundle
        val clientFromV1Bundle = Client().buildFromBundle(bundle!!)
        assertEquals(client.address, clientFromV1Bundle.address)
        assertEquals(
            client.privateKeyBundleV1?.identityKey,
            clientFromV1Bundle.privateKeyBundleV1?.identityKey
        )
        assertEquals(
            client.privateKeyBundleV1?.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1?.preKeysList
        )
    }

    @Test
    fun testCanBeCreatedWithV1Bundle() {
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet)
        val bundleV1 = client.v1keys
        val clientFromV1Bundle = Client().buildFromV1Bundle(bundleV1!!)
        assertEquals(client.address, clientFromV1Bundle.address)
        assertEquals(
            client.privateKeyBundleV1?.identityKey,
            clientFromV1Bundle.privateKeyBundleV1?.identityKey
        )
        assertEquals(
            client.privateKeyBundleV1?.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1?.preKeysList
        )
    }

    @Test
    fun testExistingWallet() {
        // Generated from JS script
        val ints = arrayOf(
            31, 116, 198, 193, 189, 122, 19, 254, 191, 189, 211, 215, 255, 131,
            171, 239, 243, 33, 4, 62, 143, 86, 18, 195, 251, 61, 128, 90, 34, 126, 219, 236
        )
        val bytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }
        val key = PrivateKey.newBuilder().also {
            it.secp256K1Builder.bytes = bytes.toByteString()
            it.publicKeyBuilder.secp256K1UncompressedBuilder.bytes = KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(bytes)).toByteString()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val conversations = client.conversations.list()
        assertEquals(1, conversations.size)
        val message = conversations[0].messages().firstOrNull()
        assertEquals(message?.body, "hello")
    }
}
