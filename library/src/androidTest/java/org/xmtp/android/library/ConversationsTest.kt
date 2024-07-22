package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Ignore
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.Signature
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.consentProofText
import org.xmtp.android.library.messages.createDeterministic
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.rawDataWithNormalizedRecovery
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.ConsentProofPayload
import java.lang.Thread.sleep
import java.util.Date

@RunWith(AndroidJUnit4::class)
class ConversationsTest {
    lateinit var alixWallet: PrivateKeyBuilder
    lateinit var boWallet: PrivateKeyBuilder
    lateinit var alix: PrivateKey
    lateinit var alixClient: Client
    lateinit var bo: PrivateKey
    lateinit var boClient: Client
    lateinit var caroClient: Client
    lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        fixtures = fixtures()
        alixWallet = fixtures.aliceAccount
        alix = fixtures.alice
        boWallet = fixtures.bobAccount
        bo = fixtures.bob
        alixClient = fixtures.aliceClient
        boClient = fixtures.bobClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanGetConversationFromIntroEnvelope() {
        val created = Date()
        val newWallet = PrivateKeyBuilder()
        val newClient = runBlocking { Client().create(account = newWallet) }
        val message = MessageV1Builder.buildEncode(
            sender = newClient.privateKeyBundleV1,
            recipient = fixtures.aliceClient.v1keys.toPublicKeyBundle(),
            message = TextCodec().encode(content = "hello").toByteArray(),
            timestamp = created
        )
        val envelope = EnvelopeBuilder.buildFromTopic(
            topic = Topic.userIntro(alixClient.address),
            timestamp = created,
            message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
        )
        val conversation = alixClient.conversations.fromIntro(envelope = envelope)
        assertEquals(conversation.peerAddress, newWallet.address)
        assertEquals(conversation.createdAt.time, created.time)
    }

    @Test
    fun testCanGetConversationFromInviteEnvelope() {
        val created = Date()
        val newWallet = PrivateKeyBuilder()
        val newClient = runBlocking { Client().create(account = newWallet) }
        val invitation = InvitationV1.newBuilder().build().createDeterministic(
            sender = newClient.keys,
            recipient = alixClient.keys.getPublicKeyBundle()
        )
        val sealed = SealedInvitationBuilder.buildFromV1(
            sender = newClient.keys,
            recipient = alixClient.keys.getPublicKeyBundle(),
            created = created,
            invitation = invitation
        )
        val peerAddress = alix.walletAddress
        val envelope = EnvelopeBuilder.buildFromTopic(
            topic = Topic.userInvite(peerAddress),
            timestamp = created,
            message = sealed.toByteArray()
        )
        val conversation = alixClient.conversations.fromInvite(envelope = envelope)
        assertEquals(conversation.peerAddress, newWallet.address)
        assertEquals(conversation.createdAt.time, created.time)
    }

    @Test
    fun testStreamAllMessages() {
        val boConversation =
            runBlocking { boClient.conversations.newConversation(alixClient.address) }

        // Record message stream across all conversations
        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        sleep(2500)

        for (i in 0 until 5) {
            runBlocking { boConversation.send(text = "Message $i") }
            sleep(1000)
        }
        assertEquals(5, allMessages.size)

        val caroConversation =
            runBlocking { caroClient.conversations.newConversation(alixClient.address) }
        sleep(2500)

        for (i in 0 until 5) {
            runBlocking { caroConversation.send(text = "Message $i") }
            sleep(1000)
        }

        assertEquals(10, allMessages.size)

        job.cancel()

        CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        sleep(2500)

        for (i in 0 until 5) {
            runBlocking { boConversation.send(text = "Message $i") }
            sleep(1000)
        }

        assertEquals(15, allMessages.size)
    }

    @Test
    fun testStreamTimeOutsAllMessages() {
        val boConversation =
            runBlocking { boClient.conversations.newConversation(alixClient.address) }

        // Record message stream across all conversations
        val allMessages = mutableListOf<DecodedMessage>()

        val job = CoroutineScope(Dispatchers.IO).launch {
            try {
                alixClient.conversations.streamAllMessages().collect { message ->
                    allMessages.add(message)
                }
            } catch (e: Exception) {
            }
        }
        sleep(2500)

        runBlocking { boConversation.send(text = "first message") }
        sleep(2000)
        assertEquals(allMessages.size, 1)
        sleep(121000)
        runBlocking { boConversation.send(text = "second message") }
        sleep(2000)
        assertEquals(allMessages.size, 2)
    }

    @Test
    @Ignore("TODO: Fix Flaky Test")
    fun testSendConversationWithConsentSignature() {
        val timestamp = Date().time
        val signatureClass = Signature.newBuilder().build()
        val signatureText = signatureClass.consentProofText(boClient.address, timestamp)
        val signature = runBlocking { alixWallet.sign(signatureText) }
        val hex = signature.rawDataWithNormalizedRecovery.toHex()
        val consentProofPayload = ConsentProofPayload.newBuilder().also {
            it.signature = hex
            it.timestamp = timestamp
            it.payloadVersion = Invitation.ConsentProofPayloadVersion.CONSENT_PROOF_PAYLOAD_VERSION_1
        }.build()
        val boConversation =
            runBlocking { boClient.conversations.newConversation(alixClient.address, null, consentProofPayload) }
        val alixConversations = runBlocking {
            alixClient.conversations.list()
        }
        val alixConversation = alixConversations.find {
            it.topic == boConversation.topic
        }
        assertNotNull("Alix Conversation should exist " + alixConversations.size, alixConversation)
//        Commenting out for now, the signature being created is not valid
        val isAllowed = runBlocking { alixClient.contacts.isAllowed(boClient.address) }
        assertTrue(isAllowed)
    }

    @Test
    @Ignore("TODO: Fix Flaky Test")
    fun testNetworkConsentOverConsentProof() {
        val timestamp = Date().time
        val signatureText = Signature.newBuilder().build().consentProofText(boClient.address, timestamp)
        val signature = runBlocking { alixWallet.sign(signatureText) }
        val hex = signature.rawDataWithNormalizedRecovery.toHex()
        val consentProofPayload = ConsentProofPayload.newBuilder().also {
            it.signature = hex
            it.timestamp = timestamp
            it.payloadVersion = Invitation.ConsentProofPayloadVersion.CONSENT_PROOF_PAYLOAD_VERSION_1
        }.build()
        runBlocking { alixClient.contacts.deny(listOf(boClient.address)) }
        val boConversation = runBlocking { boClient.conversations.newConversation(alixClient.address, null, consentProofPayload) }
        val alixConversations = runBlocking { alixClient.conversations.list() }
        val alixConversation = alixConversations.find { it.topic == boConversation.topic }
        assertNotNull(alixConversation)
        val isDenied = runBlocking { alixClient.contacts.isDenied(boClient.address) }
        assertTrue(isDenied)
    }

    @Test
    @Ignore("TODO: Fix Flaky Test")
    fun testConsentProofInvalidSignature() {
        val timestamp = Date().time
        val signatureText =
            Signature.newBuilder().build().consentProofText(boClient.address, timestamp + 1)
        val signature = runBlocking { alixWallet.sign(signatureText) }
        val hex = signature.rawDataWithNormalizedRecovery.toHex()
        val consentProofPayload = ConsentProofPayload.newBuilder().also {
            it.signature = hex
            it.timestamp = timestamp
            it.payloadVersion =
                Invitation.ConsentProofPayloadVersion.CONSENT_PROOF_PAYLOAD_VERSION_1
        }.build()

        val boConversation = runBlocking {
            boClient.conversations.newConversation(
                alixClient.address,
                null,
                consentProofPayload
            )
        }
        val alixConversations = runBlocking { alixClient.conversations.list() }
        val alixConversation = alixConversations.find { it.topic == boConversation.topic }
        assertNotNull("Alix conversation should exist" + alixConversations.size, alixConversation)
        val isAllowed = runBlocking { alixClient.contacts.isAllowed(boClient.address) }
        assertFalse("Should not be allowed", isAllowed)
    }
}
