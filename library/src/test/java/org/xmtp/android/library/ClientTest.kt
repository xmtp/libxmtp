package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
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
    fun testCanMessage() {
        val fixtures = fixtures()
        val notOnNetwork = PrivateKeyBuilder()
        val canMessage = fixtures.aliceClient.canMessage(fixtures.bobClient.address)
        val cannotMessage = fixtures.aliceClient.canMessage(notOnNetwork.address)
        assert(canMessage)
        assert(!cannotMessage)
    }
}
