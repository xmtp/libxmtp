package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.protobuf.kotlin.toByteString
import com.google.protobuf.kotlin.toByteStringUtf8
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.Assert.assertEquals
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.web3j.crypto.Hash
import org.web3j.utils.Numeric
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundle
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.PublicKeyBundle
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.SignedPublicKeyBundleBuilder
import org.xmtp.android.library.messages.createDeterministic
import org.xmtp.android.library.messages.decrypt
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.recipientAddress
import org.xmtp.android.library.messages.senderAddress
import org.xmtp.android.library.messages.sharedSecret
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.InvitationV1.Context
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.nio.charset.StandardCharsets.UTF_8
import java.util.Date

@RunWith(AndroidJUnit4::class)
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
                timestamp = Date(),
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
            InvitationV1.newBuilder().build().createDeterministic(
                sender = alice.toV2(),
                recipient = bob.toV2().getPublicKeyBundle(),
                context = invitationContext,
            )
        val sealedInvitation = SealedInvitationBuilder.buildFromV1(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle(),
            created = Date(),
            invitation = invitationv1,
        )
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = "Yo!")
        val message1 = MessageV2Builder.buildEncode(
            client = client,
            encodedContent,
            topic = invitationv1.topic,
            keyMaterial = invitationv1.aes256GcmHkdfSha256.keyMaterial.toByteArray(),
            codec = encoder,
        )
        val decoded = MessageV2Builder.buildDecode(
            id = "",
            client = client,
            message = message1.messageV2,
            keyMaterial = invitationv1.aes256GcmHkdfSha256.keyMaterial.toByteArray(),
            topic = invitationv1.topic,
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
        val additionalData =
            Numeric.hexStringToByteArray(
                "0aac020a940108d995eeadcc3012460a440a408f20c9fc03909edeb21538b0a568c423f8829e95c0270779ca704f72a45f02416f6071f6faaf421cac3bacc6bb432fc4b5f92bc4391349953c7c98f12253cdd710011a430a4104b7eb7b56059a4f08bf3dd8f1b329e21d486e39822f17db15bad0d7f689f6c8081ae2800b9014fc9ef355a39e10503fddfdfa0b07ccc1946c2275b10e660d5ded12920108e995eeadcc3012440a420a40da669aa014468ffe34d5b962443d8b1e353b1e39f252bbcffa5c6c70adf9f7d2484de944213f345bac869e8c1942657b9c59f6fc12d139171b22789bc76ffb971a430a4104901d3a7f728bde1f871bcf46d44dcf34eead4c532135913583268d35bd93ca0a1571a8cb6546ab333f2d77c3bb9839be7e8f27795ea4d6e979b6670dec20636d12aa020a920108bad3eaadcc3012440a420a4016d83a6e44ee8b9764f18fbb390f2a4049d92ff904ebd75c76a71d58a7f943744f8bed7d3696f9fb41ce450c5ab9f4a7f9a83e3d10f401bbe85e3992c5156d491a430a41047cebe3a23e573672363665d13220d368d37776e10232de9bd382d5af36392956dbd806f8b78bec5cdc111763e4ef4aff7dee65a8a15fee8d338c387320c5b23912920108bad3eaadcc3012440a420a404a751f28001f34a4136529a99e738279856da6b32a1ee9dba20849d9cd84b6165166a6abeae1139ed8df8be3b4594d9701309075f2b8d5d4de1f713fb62ae37e1a430a41049c45e552ac9f69c083bd358acac31a2e3cf7d9aa9298fef11b43252730949a39c68272302a61b548b13452e19272c119b5189a5d7b5c3283a37d5d9db5ed0c6818b286deaecc30",
            )
        val ciphertext = CipherText.newBuilder().apply {
            aes256GcmHkdfSha256 = aes256GcmHkdfSha256.toBuilder().also {
                it.gcmNonce = nonce.toByteString()
                it.hkdfSalt = salt.toByteString()
                it.payload = payload.toByteString()
            }.build()
        }.build()

        val decrypted = Crypto.decrypt(secret, ciphertext, additionalData = additionalData)

        assertEquals(content.toByteString(), decrypted?.toByteString())
    }

    @Test
    @Ignore("Dev network flaky should be moved to local")
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
            it.secp256K1 = it.secp256K1.toBuilder().also { builder ->
                builder.bytes = keyBytes.toByteString()
            }.build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes))
                                .toByteString()
                    }.build()
            }.build()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val convo = client.conversations.list()[0]
        val message = convo.messages()[0]
        assertEquals("Test message", message.content())
    }

    @Test
    @Ignore("Dev network flaky should be moved to local")
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
            it.secp256K1 = it.secp256K1.toBuilder().also { builder ->
                builder.bytes = keyBytes.toByteString()
            }.build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes))
                                .toByteString()
                    }.build()
            }.build()
        }.build()

        val client = Client().create(account = PrivateKeyBuilder(key))
        assertEquals(client.apiClient.environment, XMTPEnvironment.DEV)
        val convo = client.conversations.list()[0]
        runBlocking {
            convo.send(
                text = "hello deflate from kotlin again",
                SendOptions(compression = EncodedContentCompression.DEFLATE),
            )
        }
        val message = convo.messages().lastOrNull()!!
        assertEquals("hello deflate from kotlin again", message.content())
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
            it.secp256K1 = it.secp256K1.toBuilder().also { builder ->
                builder.bytes = keyBytes.toByteString()
            }.build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(keyBytes))
                                .toByteString()
                    }.build()
            }.build()
        }.build()
        val client = Client().create(account = PrivateKeyBuilder(key))
        val conversations = client.conversations.list()
        assertEquals(201, conversations.size)
    }

    @Test
    fun canReceiveV1MessagesFromJS() {
        val wallet = FakeWallet.generate()
        val client = Client().create(account = wallet)
        val convo = ConversationV1(
            client = client,
            peerAddress = "0xf4BF19Ed562651837bc11ff975472ABd239D35B5",
            sentAt = Date(),
        )
        runBlocking { convo.send(text = "hello from kotlin") }
        val messages = convo.messages()
        assertEquals(1, messages.size)
        assertEquals("hello from kotlin", messages[0].body)
        assertEquals(convo.topic.description, messages[0].topic)
    }

    @Test
    fun canReceiveV2MessagesFromJS() {
        val wallet = PrivateKeyBuilder()
        val client = Client().create(account = wallet)
        val convo = client.conversations.newConversation(
            "0xf4BF19Ed562651837bc11ff975472ABd239D35B5",
            InvitationV1ContextBuilder.buildFromConversation("https://example.com/4"),
        )

        runBlocking { convo.send(content = "hello from kotlin") }
        val messages = convo.messages()
        assertEquals(1, messages.size)
        assertEquals("hello from kotlin", messages[0].body)
        assertEquals(convo.topic, messages[0].topic)
    }

    @Test
    fun testGetsV1ID() {
        val fixtures = fixtures()
        val conversation =
            fixtures.aliceClient.conversations.newConversation(fixtures.bob.walletAddress)
        runBlocking { conversation.send(text = "hi") }
        val envelope = fixtures.fakeApiClient.published.lastOrNull()!!
        val decodedMessage = conversation.decode(envelope)
        assertEquals(Hash.sha256(envelope.message.toByteArray()).toHex(), decodedMessage.id)
    }

    @Test
    fun testGetsV2ID() {
        val envelopeMessageData =
            Numeric.hexStringToByteArray(
                "12bf040a470880dedf9dafc0ff9e17123b2f786d74702f302f6d2d32536b644e355161305a6d694649357433524662667749532d4f4c76356a7573716e6465656e544c764e672f70726f746f12f3030af0030a20439174a205643a50af33c7670341338526dbb9c1cf0560687ff8a742e957282d120c090ba2b385b40639867493ce1abd037648c947f72e5c62e8691d7748e78f9a346ff401c97a628ebecf627d722829ff9cfb7d7c3e0b9e26b5801f2b5a39fd58757cc5771427bfefad6243f52cfc84b384fa042873ebeb90948aa80ca34f26ff883d64720c9228ed6bcd1a5c46953a12ae8732fd70260651455674e2e2c23bc8d64ed35562fef4cdfc55d38e72ad9cf2d597e68f48b6909967b0f5d0b4f33c0af3efce55c739fbc93888d20b833df15811823970a356b26622936564d830434d3ecde9a013f7433142e366f1df5589131e440251be54d5d6deef9aaaa9facac26eb54fb7b74eb48c5a2a9a2e2956633b123cc5b91dec03e4dba30683be03bd7510f16103d3f81712dccf2be003f2f77f9e1f162bc47f6c1c38a1068abd3403952bef31d75e8024e7a62d9a8cbd48f1872a0156abb559d01de689b4370a28454658957061c46f47fc5594808d15753876d4b5408b3a3410d0555c016e427dfceae9c05a4a21fd7ce4cfbb11b2a696170443cf310e0083b0a48e357fc2f00c688c0b56821c8a14c2bb44ddfa31d680dfc85efe4811e86c6aa3adfc373ad5731ddab83960774d98d60075b8fd70228da5d748bfb7a5334bd07e1cc4a9fbf3d5de50860d0684bb27786b5b4e00d415",
            )
        val envelope = MessageApiOuterClass.Envelope.newBuilder().also {
            it.contentTopic = "/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto"
            it.message = envelopeMessageData.toByteString()
            it.timestampNs = Date().time * 1_000_000
        }.build()
        val ints = arrayOf(
            80, 84, 15, 126, 14, 105, 216, 8, 61, 147, 153, 232, 103, 69, 219, 13,
            99, 118, 68, 56, 160, 94, 58, 22, 140, 247, 221, 172, 14, 188, 52, 88,
        )
        val bytes =
            ints.foldIndexed(ByteArray(ints.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }
        val key = PrivateKeyOuterClass.PrivateKey.newBuilder().also {
            it.secp256K1 = it.secp256K1.toBuilder().also { builder ->
                builder.bytes = bytes.toByteString()
            }.build()
            it.publicKey = it.publicKey.toBuilder().also { builder ->
                builder.secp256K1Uncompressed =
                    builder.secp256K1Uncompressed.toBuilder().also { keyBuilder ->
                        keyBuilder.bytes =
                            KeyUtil.addUncompressedByte(KeyUtil.getPublicKey(bytes)).toByteString()
                    }.build()
            }.build()
        }.build()
        val keyBundleData =
            Numeric.hexStringToByteArray("0a86030ac001089387b882df3012220a204a393d6ac64c10770a2585def70329f10ca480517311f0b321a5cfbbae0119951a9201089387b882df3012440a420a4092f66532cf0266d146a17060fb64148e4a6adc673c14511e45f40ac66551234a336a8feb6ef3fabdf32ea259c2a3bca32b9550c3d34e004ea59e86b42f8001ac1a430a41041c919edda3399ab7f20f5e1a9339b1c2e666e80a164fb1c6d8bc1b7dbf2be158f87c837a6364c7fb667a40c2d234d198a7c2168a928d39409ad7d35d653d319912c00108a087b882df3012220a202ade2eefefa5f8855e557d685278e8717e3f57682b66c3d73aa87896766acddc1a920108a087b882df3012440a420a404f4a90ef10e1536e4588f12c2320229008d870d2abaecd1acfefe9ca91eb6f6d56b1380b1bdebdcf9c46fb19ceb3247d5d986a4dd2bce40a4bdf694c24b08fbb1a430a4104a51efe7833c46d2f683e2eb1c07811bb96ab5e4c2000a6f06124968e8842ff8be737ad7ca92b2dabb13550cdc561df15771c8494eca7b7ca5519f6da02f76489")
        val keyBundle = PrivateKeyOuterClass.PrivateKeyBundle.parseFrom(keyBundleData)
        val client = Client().buildFrom(bundle = keyBundle.v1)
        val conversationJSON =
            (""" {"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z","context":{"conversationId":"pat/messageid","metadata":{}}} """).toByteArray(
                UTF_8,
            )
        val decodedConversation = client.importConversation(conversationJSON)
        val conversation = ConversationV2(
            topic = decodedConversation.topic,
            keyMaterial = decodedConversation.keyMaterial!!,
            context = Context.newBuilder().build(),
            peerAddress = decodedConversation.peerAddress,
            client = client,
            header = Invitation.SealedInvitationHeaderV1.newBuilder().build(),
        )
        val decodedMessage = conversation.decodeEnvelope(envelope)
        assertEquals(
            decodedMessage.id,
            "e42a7dd44d0e1214824eab093cb89cfe6f666298d0af2d54fe0c914c8b72eff3",
        )
    }

    @Test
    fun testMessages() {
        val aliceWallet = PrivateKeyBuilder()
        val bobWallet = PrivateKeyBuilder()
        val alice = PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build()
            .generate(wallet = aliceWallet)
        val bob = PrivateKeyOuterClass.PrivateKeyBundleV1.newBuilder().build()
            .generate(wallet = bobWallet)
        val msg = "Hello world"
        val decrypted = msg.toByteStringUtf8().toByteArray()
        val alicePublic = alice.toPublicKeyBundle()
        val bobPublic = bob.toPublicKeyBundle()
        val aliceSecret =
            alice.sharedSecret(peer = bobPublic, myPreKey = alicePublic.preKey, isRecipient = false)
        val encrypted = Crypto.encrypt(aliceSecret, decrypted)
        val bobSecret =
            bob.sharedSecret(peer = alicePublic, myPreKey = bobPublic.preKey, isRecipient = true)
        val bobDecrypted = Crypto.decrypt(bobSecret, encrypted!!)
        val decryptedText = String(bobDecrypted!!, Charsets.UTF_8)
        Assert.assertEquals(decryptedText, msg)
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
            isRecipient = true,
        )
        val bobSharedSecret = bobPrivateBundle.sharedSecret(
            peer = alicePublicBundle,
            myPreKey = bobPublicBundle.preKey,
            isRecipient = false,
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
            isRecipient = true,
        )
        assert(secretData.contentEquals(secret))
    }
}
