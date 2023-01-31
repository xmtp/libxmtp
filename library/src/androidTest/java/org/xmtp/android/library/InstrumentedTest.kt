package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.secp256K1Uncompressed
import org.xmtp.android.library.messages.walletAddress

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
}
