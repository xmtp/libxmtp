package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.web3j.utils.Numeric
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.SealedInvitation
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.createDeterministic
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getInvitation
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.sharedSecret
import org.xmtp.android.library.messages.toPublicKeyBundle
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
        val conversations = runBlocking { client.conversations.list() }
        assertEquals(1, conversations.size)
        val message = runBlocking { conversations[0].messages().firstOrNull() }
        assertEquals(message?.body, "hello")
    }

    @Test
    fun testGenerateSealedInvitation() {
        val aliceWallet = FakeWallet.generate()
        val bobWallet = FakeWallet.generate()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val invitation = InvitationV1.newBuilder().build().createDeterministic(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle()
        )
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

    @Test
    fun testDeterministicInvite() {
        val aliceWallet = FakeWallet.generate()
        val bobWallet = FakeWallet.generate()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val makeInvite = { conversationId: String ->
            InvitationV1.newBuilder().build().createDeterministic(
                sender = alice.toV2(),
                recipient = bob.toV2().getPublicKeyBundle(),
                context = InvitationV1ContextBuilder.buildFromConversation(conversationId)
            )
        }
        // Repeatedly making the same invite should use the same topic/keys
        val original = makeInvite("example.com/conversation-foo")
        for (i in 1..10) {
            val invite = makeInvite("example.com/conversation-foo")
            assertEquals(original.topic, invite.topic)
        }
        // But when the conversationId changes then it use a new topic/keys
        val invite = makeInvite("example.com/conversation-bar")
        assertNotEquals(original.topic, invite.topic)
    }

    @Test
    fun testGeneratesKnownDeterministicTopic() {
        // address = 0xF56d1F3b1290204441Cb3843C2Cac1C2f5AEd690
        val aliceKeyData =
            Numeric.hexStringToByteArray("0x0a8a030ac20108c192a3f7923112220a2068d2eb2ef8c50c4916b42ce638c5610e44ff4eb3ecb098c9dacf032625c72f101a940108c192a3f7923112460a440a40fc9822283078c323c9319c45e60ab42c65f6e1744ed8c23c52728d456d33422824c98d307e8b1c86a26826578523ba15fe6f04a17fca176664ee8017ec8ba59310011a430a410498dc2315dd45d99f5e900a071e7b56142de344540f07fbc73a0f9a5d5df6b52eb85db06a3825988ab5e04746bc221fcdf5310a44d9523009546d4bfbfbb89cfb12c20108eb92a3f7923112220a20788be9da8e1a1a08b05f7cbf22d86980bc056b130c482fa5bd26ccb8d29b30451a940108eb92a3f7923112460a440a40a7afa25cb6f3fbb98f9e5cd92a1df1898452e0dfa1d7e5affe9eaf9b72dd14bc546d86c399768badf983f07fa7dd16eee8d793357ce6fccd676807d87bcc595510011a430a410422931e6295c3c93a5f6f5e729dc02e1754e916cb9be16d36dc163a300931f42a0cd5fde957d75c2068e1980c5f86843daf16aba8ae57e8160b8b9f0191def09e")
        val aliceKeys = PrivateKeyBundle.parseFrom(aliceKeyData).v1.toV2()

        // address = 0x3De402A325323Bb97f00cE3ad5bFAc96A11F9A34
        val bobKeyData =
            Numeric.hexStringToByteArray("0x0a88030ac001088cd68df7923112220a209057f8d813314a2aae74e6c4c30f909c1c496b6037ce32a12c613558a8e961681a9201088cd68df7923112440a420a40501ae9b4f75d5bb5bae3ca4ecfda4ede9edc5a9b7fc2d56dc7325b837957c23235cc3005b46bb9ef485f106404dcf71247097ed509635590f4b7987b833d03661a430a4104e61a7ae511567f4a2b5551221024b6932d6cdb8ecf3876ec64cf29be4291dd5428fc0301963cdf6939978846e2c35fd38fcb70c64296a929f166ef6e4e91045712c20108b8d68df7923112220a2027707399474d417bf6aae4baa3d73b285bf728353bc3e156b0e32461ebb48f8c1a940108b8d68df7923112460a440a40fb96fa38c3f013830abb61cf6b39776e0475eb1379c66013569c3d2daecdd48c7fbee945dcdbdc5717d1f4ffd342c4d3f1b7215912829751a94e3ae11007e0a110011a430a4104952b7158cfe819d92743a4132e2e3ae867d72f6a08292aebf471d0a7a2907f3e9947719033e20edc9ca9665874bd88c64c6b62c01928065f6069c5c80c699924")
        val bobKeys = PrivateKeyBundle.parseFrom(bobKeyData).v1.toV2()

        val aliceInvite = InvitationV1.newBuilder().build().createDeterministic(
            sender = aliceKeys,
            recipient = bobKeys.getPublicKeyBundle(),
            context = InvitationV1ContextBuilder.buildFromConversation("test")
        )

        assertEquals(
            aliceInvite.topic,
            "/xmtp/0/m-4b52be1e8567d72d0bc407debe2d3c7fca2ae93a47e58c3f9b5c5068aff80ec5/proto"
        )

        val bobInvite = InvitationV1.newBuilder().build().createDeterministic(
            sender = bobKeys,
            recipient = aliceKeys.getPublicKeyBundle(),
            context = InvitationV1ContextBuilder.buildFromConversation("test")
        )

        assertEquals(
            aliceInvite.topic,
            "/xmtp/0/m-4b52be1e8567d72d0bc407debe2d3c7fca2ae93a47e58c3f9b5c5068aff80ec5/proto"
        )

        assertEquals(
            bobInvite.topic,
            "/xmtp/0/m-4b52be1e8567d72d0bc407debe2d3c7fca2ae93a47e58c3f9b5c5068aff80ec5/proto"
        )
    }

    @Test
    fun testCreatesDeterministicTopicsBidirectionally() {
        val aliceWallet = FakeWallet.generate()
        val bobWallet = FakeWallet.generate()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)

        val aliceInvite = InvitationV1.newBuilder().build().createDeterministic(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle(),
            context = null
        )

        val bobInvite = InvitationV1.newBuilder().build().createDeterministic(
            sender = bob.toV2(),
            recipient = alice.toV2().getPublicKeyBundle(),
            context = null
        )

        val aliceSharedSecret = alice.sharedSecret(
            bob.toPublicKeyBundle(),
            alice.getPreKeys(0).publicKey,
            false
        )

        val bobSharedSecret = bob.sharedSecret(
            alice.toPublicKeyBundle(), bob.getPreKeys(0).publicKey,
            true
        )

        assertEquals(aliceSharedSecret.contentToString(), bobSharedSecret.contentToString())

        assertEquals(aliceInvite.topic, bobInvite.topic)
    }
}
