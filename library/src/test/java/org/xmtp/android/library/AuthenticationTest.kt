package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteStringUtf8
import junit.framework.TestCase.fail
import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.*

class AuthenticationTest {

    @Test
    fun testCreateToken() {
        val privateKey = PrivateKeyBuilder()
        val identity = PrivateKeyBuilder.privateKey.generate()
        // Prompt them to sign "XMTP : Create Identity ..."
        val authorized = privateKey.createIdentity(identity)
        // Create the `Authorization: Bearer $authToken` for API calls.
        val authToken = authorized.createAuthToken()
        val tokenData = authToken.toByteStringUtf8().toByteArray()
        val base64TokenData = com.google.crypto.tink.subtle.Base64.decode(tokenData, 0)
        if (tokenData.isEmpty() || base64TokenData.isEmpty()) {
            fail("could not get token data")
            return
        }
        val token = Token.parseFrom(base64TokenData)
        val authData = AuthData.parseFrom(token.authDataBytes)
        assertEquals(authData.walletAddr, authorized.address)
    }

    @Test
    fun testEnablingSavingAndLoadingOfStoredKeys() {
        val alice = PrivateKeyBuilder()
        val identity = PrivateKey.newBuilder().build().generate()
        val authorized = alice.createIdentity(identity)
        val bundle = authorized.toBundle
        val encryptedBundle = bundle.encrypted(alice)
        val decrypted = encryptedBundle.decrypted(alice)
        assertEquals(decrypted.v1.identityKey.secp256K1.bytes, identity.secp256K1.bytes)
        assertEquals(decrypted.v1.identityKey.publicKey, authorized.authorized)
    }
}