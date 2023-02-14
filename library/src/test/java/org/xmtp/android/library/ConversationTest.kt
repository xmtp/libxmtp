package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.web3j.crypto.Hash
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.InvitationV1ContextBuilder
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageHeaderV2Builder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.SignedContentBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.recoverWalletSignerPublicKey
import org.xmtp.android.library.messages.sign
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.toSignedPublicKeyBundle
import org.xmtp.android.library.messages.toV2
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import java.util.Date

class ConversationTest {
    lateinit var fakeApiClient: FakeApiClient
    lateinit var aliceWallet: PrivateKeyBuilder
    lateinit var bobWallet: PrivateKeyBuilder
    lateinit var alice: PrivateKey
    lateinit var aliceClient: Client
    lateinit var bob: PrivateKey
    lateinit var bobClient: Client

    private fun publishLegacyContact(client: Client) {
        val contactBundle = ContactBundle.newBuilder().apply {
            v1Builder.keyBundle = client.privateKeyBundleV1?.toPublicKeyBundle()
        }.build()
        val envelope = Envelope.newBuilder().apply {
            contentTopic = Topic.contact(client.address).description
            timestampNs = (Date().time * 1_000_000)
            message = contactBundle.toByteString()
        }.build()

        client.publish(envelopes = listOf(envelope))
    }

    @Before
    fun setUp() {
        aliceWallet = PrivateKeyBuilder()
        alice = aliceWallet.getPrivateKey()
        bobWallet = PrivateKeyBuilder()
        bob = bobWallet.getPrivateKey()
        fakeApiClient = FakeApiClient()
        aliceClient = Client().create(account = aliceWallet, apiClient = fakeApiClient)
        bobClient = Client().create(account = bobWallet, apiClient = fakeApiClient)
    }

    @Test
    fun testDoesNotAllowConversationWithSelf() {
        val client = Client().create(account = aliceWallet)
        assertThrows("Recipient is sender", XMTPException::class.java) {
            client.conversations.newConversation(alice.walletAddress)
        }
    }

    @Test
    fun testCanInitiateV2Conversation() {
        val existingConversations = aliceClient.conversations.conversations
        assert(existingConversations.isEmpty())
        val conversation = bobClient.conversations.newConversation(alice.walletAddress)
        val aliceInviteMessage =
            fakeApiClient.findPublishedEnvelope(Topic.userInvite(alice.walletAddress))
        val bobInviteMessage =
            fakeApiClient.findPublishedEnvelope(Topic.userInvite(bob.walletAddress))
        assert(aliceInviteMessage != null)
        assert(bobInviteMessage != null)
        assertEquals(conversation.peerAddress, alice.walletAddress)
        val newConversations = aliceClient.conversations.list()
        assertEquals("already had conversations somehow", 1, newConversations.size)
    }

    @Test
    fun testCanFindExistingV1Conversation() {
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = "hi alice")
        // Get a date that's roughly two weeks ago to test with
        val someTimeAgo = Date(System.currentTimeMillis() - 2_000_000)
        val messageV1 = MessageV1Builder.buildEncode(
            sender = bobClient.privateKeyBundleV1!!,
            recipient = aliceClient.privateKeyBundleV1?.toPublicKeyBundle()!!,
            message = encodedContent.toByteArray(),
            timestamp = someTimeAgo
        )
        // Overwrite contact as legacy
        bobClient.publishUserContact(legacy = true)
        aliceClient.publishUserContact(legacy = true)
        bobClient.publish(
            envelopes = listOf(
                EnvelopeBuilder.buildFromTopic(
                    topic = Topic.userIntro(bob.walletAddress),
                    timestamp = someTimeAgo,
                    message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray()
                ),
                EnvelopeBuilder.buildFromTopic(
                    topic = Topic.userIntro(alice.walletAddress),
                    timestamp = someTimeAgo,
                    message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray()
                ),
                EnvelopeBuilder.buildFromTopic(
                    topic = Topic.directMessageV1(
                        bob.walletAddress,
                        alice.walletAddress
                    ),
                    timestamp = someTimeAgo,
                    message = MessageBuilder.buildFromMessageV1(v1 = messageV1).toByteArray()
                )
            )
        )
        var conversation = aliceClient.conversations.newConversation(bob.walletAddress)
        assertEquals(conversation.peerAddress, bob.walletAddress)
        assertEquals(conversation.createdAt, someTimeAgo)
        val existingMessages = fakeApiClient.published.size
        conversation = bobClient.conversations.newConversation(alice.walletAddress)

        assertEquals(
            "published more messages when we shouldn't have",
            existingMessages,
            fakeApiClient.published.size
        )
        assertEquals(conversation.peerAddress, alice.walletAddress)
        assertEquals(conversation.createdAt, someTimeAgo)
    }

    @Test
    fun testCanFindExistingV2Conversation() {
        val existingConversation = bobClient.conversations.newConversation(
            alice.walletAddress,
            context = InvitationV1ContextBuilder.buildFromConversation("http://example.com/2")
        )
        var conversation: Conversation? = null
        fakeApiClient.assertNoPublish {
            conversation = bobClient.conversations.newConversation(
                alice.walletAddress,
                context = InvitationV1ContextBuilder.buildFromConversation("http://example.com/2")
            )
        }
        assertEquals(
            "made new conversation instead of using existing one",
            conversation!!.topic,
            existingConversation.topic
        )
    }

    @Test
    fun testCanLoadV1Messages() {
        // Overwrite contact as legacy so we can get v1
        publishLegacyContact(client = bobClient)
        publishLegacyContact(client = aliceClient)
        val bobConversation = bobClient.conversations.newConversation(aliceWallet.address)
        val aliceConversation = aliceClient.conversations.newConversation(bobWallet.address)

        bobConversation.send(content = "hey alice")
        bobConversation.send(content = "hey alice again")
        val messages = aliceConversation.messages()
        assertEquals(2, messages.size)
        assertEquals("hey alice", messages[1].body)
        assertEquals(bobWallet.address, messages[1].senderAddress)
    }

    @Test
    fun testCanLoadV2Messages() {
        val bobConversation = bobClient.conversations.newConversation(
            aliceWallet.address,
            InvitationV1ContextBuilder.buildFromConversation("hi")
        )

        val aliceConversation = aliceClient.conversations.newConversation(
            bobWallet.address,
            InvitationV1ContextBuilder.buildFromConversation("hi")
        )
        bobConversation.send(content = "hey alice")
        val messages = aliceConversation.messages()
        assertEquals(1, messages.size)
        assertEquals("hey alice", messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testVerifiesV2MessageSignature() {
        val aliceConversation = aliceClient.conversations.newConversation(
            bobWallet.address,
            context = InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
        )

        val codec = TextCodec()
        val originalContent = codec.encode(content = "hello")
        val tamperedContent = codec.encode(content = "this is a fake")
        val originalPayload = originalContent.toByteArray()
        val tamperedPayload = tamperedContent.toByteArray()
        val date = Date()
        val header = MessageHeaderV2Builder.buildFromTopic(aliceConversation.topic, created = date)
        val headerBytes = header.toByteArray()
        val digest = Hash.sha256(headerBytes + tamperedPayload)
        val preKey = aliceClient.keys?.preKeysList?.get(0)
        val signature = preKey?.sign(digest)
        val bundle = aliceClient.privateKeyBundleV1?.toV2()?.getPublicKeyBundle()
        val signedContent =
            SignedContentBuilder.builderFromPayload(
                payload = originalPayload,
                sender = bundle,
                signature = signature
            )
        val signedBytes = signedContent.toByteArray()
        val ciphertext =
            Crypto.encrypt(
                aliceConversation.keyMaterial!!,
                signedBytes,
                additionalData = headerBytes
            )
        val tamperedMessage =
            MessageV2Builder.buildFromCipherText(headerBytes = headerBytes, ciphertext = ciphertext)
        aliceClient.publish(
            envelopes = listOf(
                EnvelopeBuilder.buildFromString(
                    topic = aliceConversation.topic,
                    timestamp = Date(),
                    message = MessageBuilder.buildFromMessageV2(v2 = tamperedMessage).toByteArray()
                )
            )
        )
        val bobConversation = bobClient.conversations.newConversation(
            aliceWallet.address,
            InvitationV1ContextBuilder.buildFromConversation("hi")
        )
        assertThrows("Invalid signature", XMTPException::class.java) {
            val messages = bobConversation.messages()
        }
    }

    @Test
    fun testCanSendGzipCompressedV1Messages() {
        publishLegacyContact(client = bobClient)
        publishLegacyContact(client = aliceClient)
        val bobConversation = bobClient.conversations.newConversation(aliceWallet.address)
        val aliceConversation = aliceClient.conversations.newConversation(bobWallet.address)
        bobConversation.send(
            text = MutableList(1000) { "A" }.toString(),
            sendOptions = SendOptions(compression = EncodedContentCompression.GZIP)
        )
        val messages = aliceConversation.messages()
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].content())
    }

    @Test
    fun testCanSendDeflateCompressedV1Messages() {
        publishLegacyContact(client = bobClient)
        publishLegacyContact(client = aliceClient)
        val bobConversation = bobClient.conversations.newConversation(aliceWallet.address)
        val aliceConversation = aliceClient.conversations.newConversation(bobWallet.address)
        bobConversation.send(
            content = MutableList(1000) { "A" }.toString(),
            options = SendOptions(compression = EncodedContentCompression.DEFLATE)
        )
        val messages = aliceConversation.messages()
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].content())
    }

    @Test
    fun testCanSendGzipCompressedV2Messages() {
        val bobConversation = bobClient.conversations.newConversation(
            aliceWallet.address,
            InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
        )
        val aliceConversation = aliceClient.conversations.newConversation(
            bobWallet.address,
            InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
        )
        bobConversation.send(
            text = MutableList(1000) { "A" }.toString(),
            sendOptions = SendOptions(compression = EncodedContentCompression.GZIP)
        )
        val messages = aliceConversation.messages()
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testCanSendDeflateCompressedV2Messages() {
        val bobConversation = bobClient.conversations.newConversation(
            aliceWallet.address,
            InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
        )
        val aliceConversation = aliceClient.conversations.newConversation(
            bobWallet.address,
            InvitationV1ContextBuilder.buildFromConversation(conversationId = "hi")
        )
        bobConversation.send(
            content = MutableList(1000) { "A" }.toString(),
            options = SendOptions(compression = EncodedContentCompression.DEFLATE)
        )
        val messages = aliceConversation.messages()
        assertEquals(1, messages.size)
        assertEquals(MutableList(1000) { "A" }.toString(), messages[0].body)
        assertEquals(bobWallet.address, messages[0].senderAddress)
    }

    @Test
    fun testEndToEndConversation() {
        val fakeContactWallet = PrivateKeyBuilder()
        val fakeContactClient = Client().create(account = fakeContactWallet)
        fakeContactClient.publishUserContact()
        val fakeWallet = PrivateKeyBuilder()
        val client = Client().create(account = fakeWallet)
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
}
