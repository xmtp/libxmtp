package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.FlakyTest
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleV1Builder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.encrypted
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.secp256K1Uncompressed
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryRequest
import org.xmtp.proto.message.contents.Contact
import org.xmtp.proto.message.contents.InvitationV1Kt.context
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.util.Date

@RunWith(AndroidJUnit4::class)
class LocalInstrumentedTest {
    @Test
    fun testPublishingAndFetchingContactBundlesWithWhileGeneratingKeys() {
        val aliceWallet = PrivateKeyBuilder()
        val alicePrivateKey = aliceWallet.getPrivateKey()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false, appVersion = "XMTPTest/v1.0.0"))
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
            runBlocking { api.queryTopic(topic = Topic.userPrivateStoreKeyBundle(authorized.address)) }
        assertEquals(result.envelopesList.size, 1)
    }

    @Test
    @FlakyTest
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

    @Test
    fun testCanPaginateV2Messages() {
        val bob = PrivateKeyBuilder()
        val alice = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val bobClient = Client().create(bob, clientOptions)
        // Publish alice's contact
        Client().create(account = alice, clientOptions)
        val convo = bobClient.conversations.newConversation(
            alice.address,
            context = InvitationV1ContextBuilder.buildFromConversation("hi")
        )
        // Say this message is sent in the past
        val date = Date()
        date.time = date.time - 5000
        convo.send(text = "10 seconds ago", sentAt = date)
        Thread.sleep(5000)
        convo.send(text = "now")
        val messages = convo.messages(limit = 1)
        assertEquals(1, messages.size)
        val nowMessage = messages[0]
        assertEquals("now", nowMessage.body)
        val messages2 = convo.messages(limit = 1, before = nowMessage.sent)
        assertEquals(1, messages2.size)
        val tenSecondsAgoMessage = messages2[0]
        assertEquals("10 seconds ago", tenSecondsAgoMessage.body)
        val messages3 = convo.messages(limit = 1, after = tenSecondsAgoMessage.sent)
        assertEquals(1, messages3.size)
        val nowMessage2 = messages3[0]
        assertEquals("now", nowMessage2.body)
    }

    @Test
    fun testListingConversations() {
        val alice = Client().create(
            PrivateKeyBuilder(),
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        )
        val bob = Client().create(
            PrivateKeyBuilder(),
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        )

        // First Bob starts a conversation with Alice
        val c1 = bob.conversations.newConversation(
            alice.address,
            context = context {
                conversationId = "example.com/alice-bob-1"
                metadata["title"] = "First Chat"
            }
        )
        c1.send("hello Alice!")
        delayToPropagate()

        // So Alice should see just that one conversation.
        var aliceConvoList = alice.conversations.list()
        assertEquals(1, aliceConvoList.size)
        assertEquals("example.com/alice-bob-1", aliceConvoList[0].conversationId)

        // And later when Bob starts a second conversation with Alice
        val c2 = bob.conversations.newConversation(
            alice.address,
            context = context {
                conversationId = "example.com/alice-bob-2"
                metadata["title"] = "Second Chat"
            }
        )
        c2.send("hello again Alice!")
        delayToPropagate()

        // Then Alice should see both conversations, the newer one first.
        aliceConvoList = alice.conversations.list()
        assertEquals(2, aliceConvoList.size)
        assertEquals("example.com/alice-bob-2", aliceConvoList[0].conversationId)
        assertEquals("example.com/alice-bob-1", aliceConvoList[1].conversationId)
    }

    @Test
    fun testCanPaginateV1Messages() {
        val bob = PrivateKeyBuilder()
        val alice = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val bobClient = Client().create(bob, clientOptions)
        // Publish alice's contact
        Client().create(account = alice, clientOptions)
        val convo = ConversationV1(client = bobClient, peerAddress = alice.address, sentAt = Date())
        // Say this message is sent in the past
        convo.send(text = "10 seconds ago")
        Thread.sleep(10000)
        convo.send(text = "now")
        val allMessages = convo.messages()
        val messages = convo.messages(limit = 1)
        assertEquals(1, messages.size)
        val nowMessage = messages[0]
        assertEquals("now", nowMessage.body)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun testStreamAllMessagesWorksWithInvites() {
        val bob = PrivateKeyBuilder()
        val alice = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val bobClient = Client().create(bob, clientOptions)
        val aliceClient = Client().create(alice, clientOptions)
        aliceClient.conversations.streamAllMessages().mapLatest {
            assertEquals("hi", it.encodedContent.content.toStringUtf8())
        }
        val bobConversation = bobClient.conversations.newConversation(aliceClient.address)
        bobConversation.send(text = "hi")
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun testStreamAllMessagesWorksWithIntros() = runBlocking {
        val bob = PrivateKeyBuilder()
        val alice = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val bobClient = Client().create(bob, clientOptions)
        val aliceClient = Client().create(alice, clientOptions)

        // Overwrite contact as legacy
        publishLegacyContact(client = bobClient)
        publishLegacyContact(client = aliceClient)

        aliceClient.conversations.streamAllMessages().mapLatest {
            assertEquals("hi", it.encodedContent.content.toStringUtf8())
        }
        val bobConversation = bobClient.conversations.newConversation(aliceClient.address)
        assertEquals(bobConversation.version, Conversation.Version.V1)
        bobConversation.send(text = "hi")
    }

    private fun publishLegacyContact(client: Client) {
        val contactBundle = Contact.ContactBundle.newBuilder().also {
            it.v1 = it.v1.toBuilder().apply {
                keyBundle = client.privateKeyBundleV1.toPublicKeyBundle()
            }.build()
        }.build()
        val envelope = Envelope.newBuilder().also {
            it.contentTopic = Topic.contact(client.address).description
            it.timestampNs = Date().time * 1_000_000
            it.message = contactBundle.toByteString()
        }.build()

        client.publish(envelopes = listOf(envelope))
    }

    @Test
    fun testBundleMatchesWhatJSGenerates() {
        val jsBytes = arrayOf(10, 134, 3, 10, 192, 1, 8, 212, 239, 181, 224, 235, 48, 18, 34, 10, 32, 253, 223, 55, 200, 191, 179, 50, 251, 142, 186, 142, 144, 120, 55, 133, 66, 62, 227, 207, 137, 96, 29, 252, 171, 22, 50, 211, 201, 114, 170, 219, 35, 26, 146, 1, 8, 212, 239, 181, 224, 235, 48, 18, 68, 10, 66, 10, 64, 128, 94, 43, 155, 99, 38, 128, 57, 37, 120, 14, 252, 31, 231, 47, 9, 128, 134, 90, 150, 231, 9, 36, 119, 119, 177, 93, 241, 169, 185, 104, 166, 105, 25, 244, 26, 197, 83, 94, 171, 35, 9, 189, 13, 103, 141, 68, 129, 134, 121, 23, 84, 209, 102, 56, 207, 194, 238, 9, 213, 72, 74, 220, 198, 26, 67, 10, 65, 4, 93, 157, 228, 228, 120, 5, 159, 157, 196, 163, 132, 142, 147, 218, 144, 247, 192, 180, 221, 177, 31, 97, 59, 48, 110, 204, 155, 208, 233, 140, 180, 54, 136, 127, 78, 81, 49, 185, 30, 73, 110, 43, 50, 179, 76, 230, 99, 118, 58, 150, 51, 136, 13, 188, 69, 79, 81, 135, 70, 115, 91, 58, 177, 95, 18, 192, 1, 8, 215, 150, 182, 224, 235, 48, 18, 34, 10, 32, 157, 32, 14, 227, 139, 112, 46, 218, 54, 217, 214, 220, 159, 105, 220, 13, 164, 50, 168, 234, 81, 48, 224, 112, 187, 138, 18, 160, 129, 195, 187, 30, 26, 146, 1, 8, 215, 150, 182, 224, 235, 48, 18, 68, 10, 66, 10, 64, 248, 197, 168, 69, 172, 44, 172, 107, 56, 177, 111, 167, 54, 162, 189, 76, 115, 240, 113, 202, 235, 50, 168, 137, 161, 188, 111, 139, 185, 215, 159, 145, 38, 250, 224, 77, 107, 107, 9, 226, 93, 235, 71, 215, 85, 247, 141, 14, 156, 85, 144, 200, 94, 160, 108, 190, 111, 219, 29, 61, 11, 57, 237, 156, 26, 67, 10, 65, 4, 123, 22, 77, 71, 125, 86, 127, 27, 156, 189, 27, 30, 102, 185, 38, 134, 239, 69, 53, 232, 48, 104, 70, 118, 242, 114, 201, 89, 36, 94, 133, 210, 228, 205, 1, 17, 119, 121, 20, 113, 160, 64, 102, 224, 193, 9, 76, 166, 7, 4, 155, 241, 217, 116, 135, 206, 62, 77, 216, 54, 204, 39, 24, 96)
        val bytes = jsBytes.foldIndexed(ByteArray(jsBytes.size)) { i, a, v -> a.apply { set(i, v.toByte()) } }
        val options = ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = true))
        val keys = PrivateKeyBundleV1Builder.buildFromBundle(bytes)
        Client().buildFrom(bundle = keys, options = options)
    }

    @Test
    fun testBatchQuery() {
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
        val request = QueryRequest.newBuilder().addContentTopics(Topic.userPrivateStoreKeyBundle(authorized.address).description).build()
        val result =
            runBlocking { api.batchQuery(requests = listOf(request)) }

        assertEquals(result.responsesOrBuilderList.size, 1)
    }

    // A delay to allow messages to propagate before making assertions.
    private fun delayToPropagate() {
        Thread.sleep(500)
    }
}
