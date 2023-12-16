package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder
import org.xmtp.android.library.messages.generate
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
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
    fun testSerialization() {
        val wallet = PrivateKeyBuilder()
        val v1 =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = wallet)
        val encodedData = PrivateKeyBundleV1Builder.encodeData(v1)
        val v1Copy = PrivateKeyBundleV1Builder.fromEncodedData(encodedData)
        val client = Client().buildFrom(v1Copy)
        assertEquals(
            wallet.address,
            client.address,
        )
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
            clientFromV1Bundle.privateKeyBundleV1?.identityKey,
        )
        assertEquals(
            client.privateKeyBundleV1?.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1?.preKeysList,
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
            clientFromV1Bundle.privateKeyBundleV1?.identityKey,
        )
        assertEquals(
            client.privateKeyBundleV1?.preKeysList,
            clientFromV1Bundle.privateKeyBundleV1?.preKeysList,
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

    @Test
    @Ignore("CI Issues")
    fun testPublicCanMessage() {
        val aliceWallet = PrivateKeyBuilder()
        val notOnNetwork = PrivateKeyBuilder()
        val opts = ClientOptions(ClientOptions.Api(XMTPEnvironment.LOCAL, false))
        val aliceClient = Client().create(aliceWallet, opts)
        aliceClient.ensureUserContactPublished()

        val canMessage = Client.canMessage(aliceWallet.address, opts)
        val cannotMessage = Client.canMessage(notOnNetwork.address, opts)

        assert(canMessage)
        assert(!cannotMessage)
    }

    @Test
    @Ignore("CI Issues")
    fun testPreEnableIdentityCallback() {
        val fakeWallet = PrivateKeyBuilder()
        val expectation = CompletableFuture<Unit>()

        val preEnableIdentityCallback: suspend () -> Unit = {
            expectation.complete(Unit)
        }

        val opts = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            preEnableIdentityCallback = preEnableIdentityCallback
        )

        try {
            Client().create(account = fakeWallet, options = opts)
            expectation.get(5, TimeUnit.SECONDS)
        } catch (e: Exception) {
            fail("Error: $e")
        }
    }

    @Test
    @Ignore("CI Issues")
    fun testPreCreateIdentityCallback() {
        val fakeWallet = PrivateKeyBuilder()
        val expectation = CompletableFuture<Unit>()

        val preCreateIdentityCallback: suspend () -> Unit = {
            expectation.complete(Unit)
        }

        val opts = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            preCreateIdentityCallback = preCreateIdentityCallback
        )

        try {
            Client().create(account = fakeWallet, options = opts)
            expectation.get(5, TimeUnit.SECONDS)
        } catch (e: Exception) {
            fail("Error: $e")
        }
    }
}
