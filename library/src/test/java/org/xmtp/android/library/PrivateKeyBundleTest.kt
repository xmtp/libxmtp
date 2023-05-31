package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder
import org.xmtp.android.library.messages.UnsignedPublicKey
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.proto.message.contents.PrivateKeyOuterClass

class PrivateKeyBundleTest {

    @Test
    fun testConversion() {
        val wallet = PrivateKeyBuilder()
        val v1 =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = wallet)
        val v2 = v1.toV2()
        val v2PreKeyPublic = UnsignedPublicKey.parseFrom(v2.preKeysList[0].publicKey.keyBytes)
        assertEquals(
            v1.preKeysList[0].publicKey.secp256K1Uncompressed.bytes,
            v2PreKeyPublic.secp256K1Uncompressed.bytes
        )
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
            client.address
        )
    }

    @Test
    fun testKeyBundlesAreSigned() {
        val wallet = PrivateKeyBuilder()
        val v1 =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = wallet)
        assert(v1.identityKey.publicKey.hasSignature())
        assert(v1.preKeysList[0].publicKey.hasSignature())
        assert(v1.toPublicKeyBundle().identityKey.hasSignature())
        assert(v1.toPublicKeyBundle().preKey.hasSignature())
        val v2 = v1.toV2()
        assert(v2.identityKey.publicKey.hasSignature())
        assert(v2.preKeysList[0].publicKey.hasSignature())
        assert(v2.getPublicKeyBundle().identityKey.hasSignature())
        assert(v2.getPublicKeyBundle().preKey.hasSignature())
    }
}
