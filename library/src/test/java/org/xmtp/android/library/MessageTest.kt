package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import org.junit.Assert.assertEquals
import org.junit.Test
import org.web3j.utils.Numeric
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.decrypt
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.recipientAddress
import org.xmtp.android.library.messages.senderAddress
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.util.Date

class MessageTest {

    @Test
    fun testFullyEncodesDecodesMessagesV1() {
        repeat(10) {
            val aliceWallet = PrivateKeyBuilder()
            val bobWallet = PrivateKeyBuilder()
            val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
            val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
            val content = "Yo!".toByteStringUtf8().toByteArray()
            val message1 = MessageV1Builder.buildEncode(
                sender = alice,
                recipient = bob.toPublicKeyBundle(),
                message = content,
                timestamp = Date()
            )
            assertEquals(aliceWallet.getPrivateKey().walletAddress, message1.senderAddress)
            assertEquals(bobWallet.getPrivateKey().walletAddress, message1.recipientAddress)
            val decrypted = message1.decrypt(alice)
            val text = decrypted?.toByteString()?.toStringUtf8()
            assertEquals(text, "Yo!")
        }
    }

    @Test
    fun testFullyEncodesDecodesMessagesV2() {
        val aliceWallet = PrivateKeyBuilder()
        val bobWallet = PrivateKeyBuilder()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val client = Client().create(account = aliceWallet)
        val invitationContext = Invitation.InvitationV1.Context.newBuilder().apply {
            conversationId = "https://example.com/1"
        }.build()
        val invitationv1 =
            InvitationV1.newBuilder().build().createRandom(context = invitationContext)
        val sealedInvitation = SealedInvitationBuilder.buildFromV1(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle(),
            created = Date(),
            invitation = invitationv1
        )
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = "Yo!")
        val message1 = MessageV2Builder.buildEncode(
            client = client,
            encodedContent,
            topic = invitationv1.topic,
            keyMaterial = invitationv1.aes256GcmHkdfSha256.keyMaterial.toByteArray()
        )
        val decoded = MessageV2Builder.buildDecode(
            message1,
            keyMaterial = invitationv1.aes256GcmHkdfSha256.keyMaterial.toByteArray()
        )
        val result: String? = decoded.content()
        assertEquals(result, "Yo!")
    }

    @Test
    fun testCanDecrypt() {
        // All of these values were generated from xmtp-js
        val content =
            Numeric.hexStringToByteArray("0a120a08786d74702e6f7267120474657874180112110a08656e636f64696e6712055554462d3822026869")
        val salt =
            Numeric.hexStringToByteArray("48c6c40ce9998a8684937b2bd90c492cef66c9cd92b4a30a4f811b43fd0aed79")
        val nonce = Numeric.hexStringToByteArray("31f78d2c989a37d8471a5d40")
        val secret =
            Numeric.hexStringToByteArray("04c86317929a0c223f44827dcf1290012b5e6538a54282beac85c2b16062fc8f781b52bea90e8c7c028254c6ba57ac144a56f054d569c340e73c6ff37aee4e68fc04a0fdb4e9c404f5d246a9fe2308f950f8374b0696dd98cc1c97fcbdbc54383ac862abee69c107723e1aa809cfbc587253b943476dc89c126af4f6515161a826ca04801742d6c45ee150a28f80cbcffd78a0210fe73ffdd74e4af8fd6307fb3d622d873653ca4bd47deb4711ef02611e5d64b4bcefcc481e236979af2b6156863e68")
        val payload =
            Numeric.hexStringToByteArray("d752fb09ee0390fe5902a1bd7b2f530da7e5b3a2bd91bad9df8fa284ab63327b86a59620fd3e2d2cf9183f46bd0fe75bda3caca893420c38416b1f")
        val additionalData = Numeric.hexStringToByteArray(
            "0aac020a940108d995eeadcc3012460a440a408f20c9fc03909edeb21538b0a568c423f8829e95c0270779ca704f72a45f02416f6071f6faaf421cac3bacc6bb432fc4b5f92bc4391349953c7c98f12253cdd710011a430a4104b7eb7b56059a4f08bf3dd8f1b329e21d486e39822f17db15bad0d7f689f6c8081ae2800b9014fc9ef355a39e10503fddfdfa0b07ccc1946c2275b10e660d5ded12920108e995eeadcc3012440a420a40da669aa014468ffe34d5b962443d8b1e353b1e39f252bbcffa5c6c70adf9f7d2484de944213f345bac869e8c1942657b9c59f6fc12d139171b22789bc76ffb971a430a4104901d3a7f728bde1f871bcf46d44dcf34eead4c532135913583268d35bd93ca0a1571a8cb6546ab333f2d77c3bb9839be7e8f27795ea4d6e979b6670dec20636d12aa020a920108bad3eaadcc3012440a420a4016d83a6e44ee8b9764f18fbb390f2a4049d92ff904ebd75c76a71d58a7f943744f8bed7d3696f9fb41ce450c5ab9f4a7f9a83e3d10f401bbe85e3992c5156d491a430a41047cebe3a23e573672363665d13220d368d37776e10232de9bd382d5af36392956dbd806f8b78bec5cdc111763e4ef4aff7dee65a8a15fee8d338c387320c5b23912920108bad3eaadcc3012440a420a404a751f28001f34a4136529a99e738279856da6b32a1ee9dba20849d9cd84b6165166a6abeae1139ed8df8be3b4594d9701309075f2b8d5d4de1f713fb62ae37e1a430a41049c45e552ac9f69c083bd358acac31a2e3cf7d9aa9298fef11b43252730949a39c68272302a61b548b13452e19272c119b5189a5d7b5c3283a37d5d9db5ed0c6818b286deaecc30"
        )
        val ciphertext = CipherText.newBuilder().apply {
            aes256GcmHkdfSha256Builder.gcmNonce = nonce.toByteString()
            aes256GcmHkdfSha256Builder.hkdfSalt = salt.toByteString()
            aes256GcmHkdfSha256Builder.payload = payload.toByteString()
        }.build()

        val decrypted = Crypto.decrypt(secret, ciphertext, additionalData = additionalData)

        assertEquals(content.toByteString(), decrypted?.toByteString())
    }

    @Test
    fun testCanReadGzipCompressedMessages() {
        val ints = arrayOf(
            225, 2, 36, 98, 37, 243, 68, 234,
            42, 126, 248, 246, 126, 83, 186, 197,
            204, 186, 19, 173, 51, 0, 64, 0,
            155, 8, 249, 247, 163, 185, 124, 159,
        )
        val keyBytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }

        val key = PrivateKeyOuterClass.PrivateKey.newBuilder().also {
            it.secp256K1Builder.bytes = keyBytes.toByteString()
            it.publicKeyBuilder.secp256K1UncompressedBuilder.bytes =
                KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes)).toByteString()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val convo = client.conversations.list()[0]
        val message = convo.messages()[0]
        assertEquals("hello gzip", message.content())
    }

    @Test
    fun testCanReadZipCompressedMessages() {
        val ints = arrayOf(
            60, 45, 240, 192, 223, 2, 14, 166,
            122, 65, 231, 31, 122, 178, 158, 137,
            192, 97, 139, 83, 133, 245, 149, 250,
            25, 125, 25, 11, 203, 97, 12, 200,
        )
        val keyBytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }

        val key = PrivateKeyOuterClass.PrivateKey.newBuilder().also {
            it.secp256K1Builder.bytes = keyBytes.toByteString()
            it.publicKeyBuilder.secp256K1UncompressedBuilder.bytes =
                KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes)).toByteString()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val convo = client.conversations.list()[0]
        val message = convo.messages().lastOrNull()!!
        assertEquals("hello deflate", message.content())
        convo.send(
            text = "hello deflate from kotlin again",
            SendOptions(compression = EncodedContentCompression.DEFLATE)
        )
    }

    @Test
    fun testCanLoadAllConversations() {
        val ints = arrayOf(
            105, 207, 193, 11, 240, 115, 115, 204,
            117, 134, 201, 10, 56, 59, 52, 90,
            229, 103, 15, 66, 20, 113, 118, 137,
            44, 62, 130, 90, 30, 158, 182, 178,
        )
        val keyBytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }

        val key = PrivateKeyOuterClass.PrivateKey.newBuilder().also {
            it.secp256K1Builder.bytes = keyBytes.toByteString()
            it.publicKeyBuilder.secp256K1UncompressedBuilder.bytes =
                KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes)).toByteString()
        }.build()
        val client = Client().create(account = PrivateKeyBuilder(key))
        val conversations = client.conversations.list()
        assertEquals(100, conversations.size)
    }
}
