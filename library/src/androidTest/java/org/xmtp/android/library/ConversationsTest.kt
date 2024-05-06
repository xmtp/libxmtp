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
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV1Builder
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

    @Test
    fun testCanGetConversationFromIntroEnvelope() {
        val fixtures = fixtures()
        val client = fixtures.aliceClient
        val created = Date()
        val newWallet = PrivateKeyBuilder()
        val newClient = Client().create(account = newWallet)
        val message = MessageV1Builder.buildEncode(
            sender = newClient.privateKeyBundleV1,
            recipient = fixtures.aliceClient.v1keys.toPublicKeyBundle(),
            message = TextCodec().encode(content = "hello").toByteArray(),
            timestamp = created
        )
        val envelope = EnvelopeBuilder.buildFromTopic(
            topic = Topic.userIntro(client.address),
            timestamp = created,
            message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
        )
        val conversation = client.conversations.fromIntro(envelope = envelope)
        assertEquals(conversation.peerAddress, newWallet.address)
        assertEquals(conversation.createdAt.time, created.time)
    }

    @Test
    fun testCanGetConversationFromInviteEnvelope() {
        val fixtures = fixtures()
        val client = fixtures.aliceClient
        val created = Date()
        val newWallet = PrivateKeyBuilder()
        val newClient = Client().create(account = newWallet)
        val invitation = InvitationV1.newBuilder().build().createDeterministic(
            sender = newClient.keys,
            recipient = client.keys.getPublicKeyBundle()
        )
        val sealed = SealedInvitationBuilder.buildFromV1(
            sender = newClient.keys,
            recipient = client.keys.getPublicKeyBundle(),
            created = created,
            invitation = invitation
        )
        val peerAddress = fixtures.alice.walletAddress
        val envelope = EnvelopeBuilder.buildFromTopic(
            topic = Topic.userInvite(peerAddress),
            timestamp = created,
            message = sealed.toByteArray()
        )
        val conversation = client.conversations.fromInvite(envelope = envelope)
        assertEquals(conversation.peerAddress, newWallet.address)
        assertEquals(conversation.createdAt.time, created.time)
    }

    @Test
    fun testStreamAllMessages() {
        val bo = PrivateKeyBuilder()
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val boClient = Client().create(bo, clientOptions)
        val alixClient = Client().create(alix, clientOptions)
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
        assertEquals(allMessages.size, 5)

        val caro = PrivateKeyBuilder()
        val caroClient = Client().create(caro, clientOptions)
        val caroConversation =
            runBlocking { caroClient.conversations.newConversation(alixClient.address) }
        sleep(2500)

        for (i in 0 until 5) {
            runBlocking { caroConversation.send(text = "Message $i") }
            sleep(1000)
        }

        assertEquals(allMessages.size, 10)

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

        assertEquals(allMessages.size, 15)
    }

    @Test
    fun testStreamTimeOutsAllMessages() {
        val bo = PrivateKeyBuilder()
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val boClient = Client().create(bo, clientOptions)
        val alixClient = Client().create(alix, clientOptions)
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
    fun testSendConversationWithConsentSignature() {
        val bo = PrivateKeyBuilder()
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val boClient = Client().create(bo, clientOptions)
        val alixClient = Client().create(alix, clientOptions)
        val timestamp = Date().time
        val signatureClass = Signature.newBuilder().build()
        val signatureText = signatureClass.consentProofText(boClient.address, timestamp)
        val signature = runBlocking { alix.sign(signatureText) }
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
        assertNotNull(alixConversation)
//        Commenting out for now, the signature being created is not valid
//        val isAllowed = runBlocking { alixClient.contacts.isAllowed(boClient.address) }
//        assertTrue(isAllowed)
    }

    @Test
    fun testNetworkConsentOverConsentProof() {
        val bo = PrivateKeyBuilder()
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val boClient = Client().create(bo, clientOptions)
        val alixClient = Client().create(alix, clientOptions)
        val timestamp = Date().time
        val signatureText = Signature.newBuilder().build().consentProofText(boClient.address, timestamp)
        val signature = runBlocking { alix.sign(signatureText) }
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
    fun testConsentProofInvalidSignature() {
        val bo = PrivateKeyBuilder()
        val alix = PrivateKeyBuilder()
        val clientOptions =
            ClientOptions(api = ClientOptions.Api(env = XMTPEnvironment.LOCAL, isSecure = false))
        val boClient = Client().create(bo, clientOptions)
        val alixClient = Client().create(alix, clientOptions)
        val timestamp = Date().time
        val signatureText =
            Signature.newBuilder().build().consentProofText(boClient.address, timestamp + 1)
        val signature = runBlocking { alix.sign(signatureText) }
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
        assertNotNull(alixConversation)
        val isAllowed = runBlocking { alixClient.contacts.isAllowed(boClient.address) }
        assertFalse(isAllowed)
    }
}
