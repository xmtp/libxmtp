package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteString
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.SealedInvitation
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getInvitation
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.toV2
import java.util.Date

@RunWith(AndroidJUnit4::class)
class InvitationTest {
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
            it.secp256K1 =
                it.secp256K1.toBuilder().also { builder -> builder.bytes = bytes.toByteString() }
                    .build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(bytes)).toByteString()
                    }.build()
            }.build()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        Assert.assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val conversations = client.conversations.list()
        Assert.assertEquals(1, conversations.size)
        val message = conversations[0].messages().firstOrNull()
        Assert.assertEquals(message?.body, "hello")
    }
    @Test
    fun testGenerateSealedInvitation() {
        val aliceWallet = FakeWallet.generate()
        val bobWallet = FakeWallet.generate()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val invitation = InvitationV1.newBuilder().build().createRandom()
        val newInvitation = SealedInvitationBuilder.buildFromV1(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle(),
            created = Date(),
            invitation = invitation
        )
        val deserialized = SealedInvitation.parseFrom(newInvitation.toByteArray())
        assert(!deserialized.v1.headerBytes.isEmpty)
        assertEquals(newInvitation, deserialized)
        val header = newInvitation.v1.header
        // Ensure the headers haven't been mangled
        assertEquals(header.sender, alice.toV2().getPublicKeyBundle())
        assertEquals(header.recipient, bob.toV2().getPublicKeyBundle())
        // Ensure alice can decrypt the invitation
        val aliceInvite = newInvitation.v1.getInvitation(viewer = alice.toV2())
        assertEquals(aliceInvite.topic, invitation.topic)
        assertEquals(
            aliceInvite.aes256GcmHkdfSha256.keyMaterial,
            invitation.aes256GcmHkdfSha256.keyMaterial
        )
        // Ensure bob can decrypt the invitation
        val bobInvite = newInvitation.v1.getInvitation(viewer = bob.toV2())
        assertEquals(bobInvite.topic, invitation.topic)
        assertEquals(
            bobInvite.aes256GcmHkdfSha256.keyMaterial,
            invitation.aes256GcmHkdfSha256.keyMaterial
        )
    }
}
