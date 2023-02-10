package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.PrivateKeyBundleBuilder
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.encrypted
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.secp256K1Uncompressed
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toSignedPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.PrivateKeyOuterClass
import java.util.Date

@RunWith(AndroidJUnit4::class)
class InstrumentedTest {
    fun publishLegacyContact(client: Client) {
        val contactBundle = ContactBundle.newBuilder().also {
            it.v1Builder.keyBundle = client.privateKeyBundleV1?.toPublicKeyBundle()
        }.build()
        val envelope = Envelope.newBuilder().also {
            it.contentTopic = Topic.contact(client.address).description
            it.timestampNs = Date().time * 1_000_000
            it.message = contactBundle.toByteString()
        }.build()

        client.publish(envelopes = listOf(envelope))
    }

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

    @Test
    fun testEndToEndConversation() {
        val options =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val fakeContactWallet = PrivateKeyBuilder()
        val fakeContactClient = Client().create(account = fakeContactWallet, options = options)
        fakeContactClient.publishUserContact()
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet, options = options)
        val contact = client.getUserContact(peerAddress = fakeContactWallet.address)!!
        assertEquals(contact.walletAddress, fakeContactWallet.address)
        val created = Date()
        val invitationContext = Invitation.InvitationV1.Context.newBuilder().also {
            it.conversationId = "https://example.com/1"
        }.build()
        val invitationv1 =
            InvitationV1.newBuilder().build().createRandom(context = invitationContext)
        val senderBundle = client.privateKeyBundleV1?.toV2()
        assertEquals(
            senderBundle?.identityKey?.publicKey?.recoverWalletSignerPublicKey()?.walletAddress,
            fakeWallet.address
        )
        val invitation = SealedInvitationBuilder.buildFromV1(
            sender = client.privateKeyBundleV1!!.toV2(),
            recipient = contact.toSignedPublicKeyBundle(),
            created = created,
            invitation = invitationv1
        )
        val inviteHeader = invitation.v1.header
        assertEquals(inviteHeader.sender.walletAddress, fakeWallet.address)
        assertEquals(inviteHeader.recipient.walletAddress, fakeContactWallet.address)
        val header = SealedInvitationHeaderV1.parseFrom(invitation.v1.headerBytes)
        val conversation =
            ConversationV2.create(client = client, invitation = invitationv1, header = header)
        assertEquals(fakeContactWallet.address, conversation.peerAddress)

        conversation.send(content = "hello world")

        val conversationList = client.conversations.list()
        val recipientConversation = conversationList.lastOrNull()

        val messages = recipientConversation?.messages()
        val message = messages?.firstOrNull()
        if (message != null) {
            assertEquals("hello world", message.body)
        }
    }

    @Test
    @Ignore
    fun testCanReceiveV1MessagesFromJS() {
        val wallet = TestHelpers.FakeWallet.generate()
        val options =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val client = Client().create(account = wallet, options = options)
        val convo = ConversationV1(
            client = client,
            peerAddress = "0xf4BF19Ed562651837bc11ff975472ABd239D35B5",
            sentAt = Date()
        )
        convo.send(text = "hello from kotlin")
        Thread.sleep(1_000)
        val messages = convo.messages()
        assertEquals(1, messages.size)
        assertEquals("hello from kotlin", messages[0].body)
    }

    @Test
    @Ignore
    fun testCanReceiveV2MessagesFromJS() {
        val wallet = PrivateKeyBuilder()
        val options =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val client = Client().create(account = wallet, options = options)
        client.publishUserContact()
        val convo = client.conversations.newConversation(
            "0xf4BF19Ed562651837bc11ff975472ABd239D35B5",
            InvitationV1ContextBuilder.buildFromConversation("https://example.com/4")
        )

        convo.send(content = "hello from kotlin")
        Thread.sleep(1_000)
        val messages = convo.messages()
        assertEquals(1, messages.size)
        assertEquals("hello from kotlin", messages[0].body)
    }
}
