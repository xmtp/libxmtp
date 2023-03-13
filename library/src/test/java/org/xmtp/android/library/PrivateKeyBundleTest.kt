package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import org.web3j.utils.Numeric
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder
import org.xmtp.android.library.messages.PublicKeyBundle
import org.xmtp.android.library.messages.SignedPublicKeyBundleBuilder
import org.xmtp.android.library.messages.UnsignedPublicKey
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.sharedSecret
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

    @Test
    fun testSharedSecret() {
        val alice = PrivateKeyBuilder()
        val alicePrivateBundle =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = alice)
                .toV2()
        val alicePublicBundle = alicePrivateBundle.getPublicKeyBundle()
        val bob = PrivateKeyBuilder()
        val bobPrivateBundle =
            PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build().generate(wallet = bob)
                .toV2()
        val bobPublicBundle = bobPrivateBundle.getPublicKeyBundle()
        val aliceSharedSecret = alicePrivateBundle.sharedSecret(
            peer = bobPublicBundle,
            myPreKey = alicePublicBundle.preKey,
            isRecipient = true
        )
        val bobSharedSecret = bobPrivateBundle.sharedSecret(
            peer = alicePublicBundle,
            myPreKey = bobPublicBundle.preKey,
            isRecipient = false
        )
        assert(aliceSharedSecret.contentEquals(bobSharedSecret))
    }

    @Test
    fun testSharedSecretMatchesWhatJSGenerates() {
        val meBundleData =
            Numeric.hexStringToByteArray("0a86030ac00108a687b5d8cc3012220a20db73e1b4b5aeffb6cecd37526d842327730433e1751bceb5824d937f779797541a920108a687b5d8cc3012440a420a40d35c081d9ab59b3fb13e27cb03a225c7134bc4ce4ce51f80273481c31d803e1e4fa8ae43e7ec20b06a81b694ad28470f85fc971b8050867f5a4821c03a67f0e81a430a410443631548a55a60f06989ce1bc3fa43fdbe463ea4748dcb509e09fc58514c6e56edfac83e1fff5f382bc110fa066762f4b862db8df53be7d48268b3fdf649adc812c00108b787b5d8cc3012220a209e2631f34af8fc1ec0f75bd15ee4e110ac424300f39bff26c7a990a75a49ac641a920108b787b5d8cc3012440a420a40202a68a2e95d446511ecf22f5487b998989989adfc0a60e1ce201e0bab64d836066ccda987cda99c0e588babb8c334a820d6a6e360100ba7ba08e0e339a303681a430a4104c9733798111d89446264db365bc0dde54b5f9202eeb309eec2f18c572ce11e267fe91e184207676d7af5eaf2ad65de0881093623030f6096ea5bf3ecd252c482")
        val youBundleData =
            Numeric.hexStringToByteArray("0a940108c487b5d8cc3012460a440a40c51e611e662117991b19f60b6a7f6d9f08671c3d55241e959954c2e0f2ec47d15b872986d2a279ffe55df01709b000fbdcc9e85c1946876e187f90a0fd32222c10011a430a41049cccf02f766f7d4c322eeb498f2ac0283a011992fc77f9e0d5687b826aafd48d8319f48f773ec959221bf7bf7d3da4b09e59af540a633c588df2f1b6f465d6a712940108cb87b5d8cc3012460a440a40b7b0e89ce4789f6e78502357864979abe9e26cd44a36ed75578368a02cdc3bda7d56721660cb2066b76a4a6dd5a78d99df4b096cc4622a2065cf05b2f32b94be10011a430a410438f2b23a4e0f9c61e716b8cf4b23f2709d92b4feb71429a385b6878c31085384701bc787def9396b441bfb8751c042432785c352f8ee9bfb9c6cd5d6871b2d1a")
        val secretData =
            Numeric.hexStringToByteArray("049f4cd17426f9dfac528f400db858a9cbc87488879d6df5bea3595beaeb37415f1b24227e571dd4969406f366841e682795f284b54952a22b2dcff87971580fa604c0a97d550ce3ce5dac2e5469a2e3ece7232d80247a789044ebef0478c6911d63400a13090de6e8aeb4a1bcb878ca73b1d7eb13ab3012e564cfef74a8182467cc047d999bb077e5b223509fab7a08642c29359b8c3144ffa30002e45f09e4a515927f682eb71b68bd52f498d5d464c6bb14d3c07aefc86a1ab8e2528a21ffd41912")
        val meBundle = PrivateKeyBundle.parseFrom(meBundleData).v1.toV2()
        val youBundlePublic =
            SignedPublicKeyBundleBuilder.buildFromKeyBundle(PublicKeyBundle.parseFrom(youBundleData))
        val secret = meBundle.sharedSecret(
            peer = youBundlePublic,
            myPreKey = meBundle.preKeysList[0].publicKey,
            isRecipient = true
        )
        assert(secretData.contentEquals(secret))
    }
}
