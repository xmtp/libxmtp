package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.encrypted
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.secp256K1Uncompressed
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.util.Date

@RunWith(AndroidJUnit4::class)
class InstrumentedTest {
    @Test
    fun testPublishingAndFetchingContactBundlesWithWhileGeneratingKeys() {
        val aliceWallet = PrivateKeyBuilder()
        val alicePrivateKey = aliceWallet.getPrivateKey()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val client = Client().create(aliceWallet, clientOptions)
        assertEquals(XMTPEnvironment.LOCAL, client.apiClient.environment)
        runBlocking {
            client.publishUserContact()
        }
        val contact = client.getUserContact(peerAddress = alicePrivateKey.walletAddress)
        assert(
            contact?.v2?.keyBundle?.identityKey?.secp256K1Uncompressed?.bytes?.toByteArray()
                .contentEquals(client.privateKeyBundleV1?.identityKey?.publicKey?.secp256K1Uncompressed?.bytes?.toByteArray())
        )
        assert(contact?.v2?.keyBundle?.identityKey?.hasSignature() ?: false)
        assert(contact?.v2?.keyBundle?.preKey?.hasSignature() ?: false)
    }

    @Test
    fun testSaveKey() {
        val alice = PrivateKeyBuilder()
        val identity = PrivateKey.newBuilder().build().generate()
        val authorized = alice.createIdentity(identity)
        val authToken = authorized.createAuthToken()
        val api = GRPCApiClient(environment = XMTPEnvironment.LOCAL, secure = false)
        api.setAuthToken(authToken)
        val encryptedBundle = authorized.toBundle.encrypted(alice)
        val envelope = Envelope.newBuilder().also {
            it.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
            it.timestampNs = Date().time * 1_000_000
            it.message = encryptedBundle.toByteString()
        }.build()
        runBlocking {
            api.publish(envelopes = listOf(envelope))
        }
        Thread.sleep(2_000)
        val result =
            runBlocking { api.query(topics = listOf(Topic.userPrivateStoreKeyBundle(authorized.address))) }
        assertEquals(result.envelopesList.size, 1)
    }

    @Test
    fun testPublishingAndFetchingContactBundlesWithSavedKeys() {
        val aliceWallet = PrivateKeyBuilder()
        val alice = PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build()
            .generate(wallet = aliceWallet)
        // Save keys
        val identity = PrivateKeyBuilder().getPrivateKey()
        val authorized = aliceWallet.createIdentity(identity)
        val authToken = authorized.createAuthToken()
        val api = GRPCApiClient(environment = XMTPEnvironment.LOCAL, secure = false)
        api.setAuthToken(authToken)
        val encryptedBundle =
            PrivateKeyBundleBuilder.buildFromV1Key(v1 = alice).encrypted(aliceWallet)
        val envelope = Envelope.newBuilder().also {
            it.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
            it.timestampNs = Date().time * 1_000_000
            it.message = encryptedBundle.toByteString()
        }.build()
        runBlocking {
            api.publish(envelopes = listOf(envelope))
        }

        // Done saving keys
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val client = Client().create(account = aliceWallet, options = clientOptions)
        assertEquals(XMTPEnvironment.LOCAL, client.apiClient.environment)
        val contact = client.getUserContact(peerAddress = aliceWallet.address)
        assertEquals(
            contact?.v2?.keyBundle?.identityKey?.secp256K1Uncompressed,
            client.privateKeyBundleV1?.identityKey?.publicKey?.secp256K1Uncompressed
        )
        assert(contact!!.v2.keyBundle.identityKey.hasSignature())
        assert(contact.v2.keyBundle.preKey.hasSignature())
    }
}
