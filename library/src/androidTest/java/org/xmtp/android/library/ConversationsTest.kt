package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import java.util.Date

@RunWith(AndroidJUnit4::class)
class ConversationsTest {

    @Test
    fun testCanGetConversationFromIntroEnvelope() {
        val fixtures = fixtures()
        val client = fixtures.aliceClient
        val created = Date()
        val newWallet = PrivateKeyBuilder()
        val newClient = Client().create(account = newWallet, apiClient = fixtures.fakeApiClient)
        val message = MessageV1Builder.buildEncode(sender = newClient.privateKeyBundleV1, recipient = fixtures.aliceClient.v1keys.toPublicKeyBundle(), message = TextCodec().encode(content = "hello").toByteArray(), timestamp = created)
        val envelope = EnvelopeBuilder.buildFromTopic(topic = Topic.userIntro(client.address), timestamp = created, message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray())
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
        val newClient = Client().create(account = newWallet, apiClient = fixtures.fakeApiClient)
        val invitation = InvitationV1.newBuilder().build().createRandom(context = null)
        val sealed = SealedInvitationBuilder.buildFromV1(sender = newClient.keys, recipient = client.keys.getPublicKeyBundle(), created = created, invitation = invitation)
        val peerAddress = fixtures.alice.walletAddress
        val envelope = EnvelopeBuilder.buildFromTopic(topic = Topic.userInvite(peerAddress), timestamp = created, message = sealed.toByteArray())
        val conversation = client.conversations.fromInvite(envelope = envelope)
        assertEquals(conversation.peerAddress, newWallet.address)
        assertEquals(conversation.createdAt.time, created.time)
    }
}
