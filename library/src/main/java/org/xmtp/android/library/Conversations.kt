package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.SealedInvitation
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.SignedPublicKeyBundle
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.decrypt
import org.xmtp.android.library.messages.getInvitation
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.involves
import org.xmtp.android.library.messages.recipientAddress
import org.xmtp.android.library.messages.senderAddress
import org.xmtp.android.library.messages.sentAt
import org.xmtp.android.library.messages.toSignedPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Contact
import org.xmtp.proto.message.contents.Invitation
import java.util.Date

data class Conversations(
    var client: Client,
    var conversations: MutableList<Conversation> = mutableListOf()
) {

    fun newConversation(
        peerAddress: String,
        context: Invitation.InvitationV1.Context? = null
    ): Conversation {
        if (peerAddress.lowercase() == client.address?.lowercase()) {
            throw IllegalArgumentException("Recipient is sender")
        }
        val existingConversation = conversations.firstOrNull { it.peerAddress == peerAddress }
        if (existingConversation != null) {
            return existingConversation
        }
        val contact = client.contacts.find(peerAddress)
            ?: throw IllegalArgumentException("Recipient not on network")
        // See if we have an existing v1 convo
        if (context?.conversationId == null || context.conversationId == "") {
            val invitationPeers = listIntroductionPeers()
            val peerSeenAt = invitationPeers[peerAddress]
            if (peerSeenAt != null) {
                val conversation: Conversation = Conversation.V1(
                    ConversationV1(
                        client = client,
                        peerAddress = peerAddress,
                        sentAt = peerSeenAt
                    )
                )
                conversations.add(conversation)
                return conversation
            }
        }
        // If the contact is v1, start a v1 conversation
        if (Contact.ContactBundle.VersionCase.V1 == contact.versionCase && (context?.conversationId == null || context.conversationId == "")) {
            val conversation: Conversation = Conversation.V1(
                ConversationV1(
                    client = client,
                    peerAddress = peerAddress,
                    sentAt = Date()
                )
            )
            conversations.add(conversation)
            return conversation
        }
        // See if we have a v2 conversation
        for (sealedInvitation in listInvitations()) {
            if (!sealedInvitation.involves(contact)) {
                continue
            }
            val invite = sealedInvitation.v1.getInvitation(viewer = client.keys)
            if (invite.context.conversationId == context?.conversationId && invite.context.conversationId != "") {
                val conversation: Conversation = Conversation.V2(
                    ConversationV2(
                        topic = invite.topic,
                        keyMaterial = invite.aes256GcmHkdfSha256.keyMaterial.toByteArray(),
                        context = invite.context,
                        peerAddress = peerAddress,
                        client = client,
                        header = sealedInvitation.v1.header
                    )
                )
                conversations.add(conversation)
                return conversation
            }
        }
        // We don't have an existing conversation, make a v2 one
        val recipient = contact.toSignedPublicKeyBundle()
        val invitation = Invitation.InvitationV1.newBuilder().build().createRandom(context)
        val sealedInvitation =
            sendInvitation(recipient = recipient, invitation = invitation, created = Date())
        val conversationV2 = ConversationV2.create(
            client = client,
            invitation = invitation,
            header = sealedInvitation.v1.header
        )
        val conversation: Conversation = Conversation.V2(conversationV2)
        conversations.add(conversation)
        return conversation
    }

    fun list(): List<Conversation> {
        val conversations: MutableList<Conversation> = mutableListOf()
        val seenPeers = listIntroductionPeers()
        for ((peerAddress, sentAt) in seenPeers) {
            conversations.add(
                Conversation.V1(
                    ConversationV1(
                        client = client,
                        peerAddress = peerAddress,
                        sentAt = sentAt
                    )
                )
            )
        }
        val invitations = listInvitations()
        for (sealedInvitation in invitations) {
            val unsealed = sealedInvitation.v1.getInvitation(viewer = client.keys)
            val conversation = ConversationV2.create(
                client = client,
                invitation = unsealed,
                header = sealedInvitation.v1.header
            )
            conversations.add(Conversation.V2(conversation))
        }
        return conversations.filter { it.peerAddress != client.address }
    }

    private fun listIntroductionPeers(): Map<String, Date> {
        val envelopes =
            runBlocking {
                client.apiClient.query(
                    topics = listOf(
                        Topic.userIntro(
                            client.address ?: ""
                        )
                    )
                ).envelopesList
            }
        val messages = envelopes.map { envelope ->
            val message = MessageV1Builder.buildFromBytes(envelope.message.toByteArray())
            // Attempt to decrypt, just to make sure we can
            message.decrypt(client.privateKeyBundleV1)
            message
        }
        val seenPeers: MutableMap<String, Date> = mutableMapOf()
        for (message in messages) {
            val recipientAddress = message.recipientAddress
            val senderAddress = message.senderAddress
            val sentAt = message.sentAt
            val peerAddress =
                if (recipientAddress == client.address) senderAddress else recipientAddress
            val existing = seenPeers[peerAddress]
            if (existing == null) {
                seenPeers[peerAddress] = sentAt
                continue
            }
            if (existing > sentAt) {
                seenPeers[peerAddress] = sentAt
            }
        }
        return seenPeers
    }

    fun listInvitations(): List<SealedInvitation> {
        val envelopes = runBlocking {
            client.apiClient.query(
                topics = listOf(
                    Topic.userInvite(
                        client.address ?: ""
                    )
                )
            ).envelopesList
        }
        return envelopes.map { envelope ->
            SealedInvitation.parseFrom(envelope.message)
        }
    }

    fun sendInvitation(
        recipient: SignedPublicKeyBundle,
        invitation: InvitationV1,
        created: Date
    ): SealedInvitation {
        client.keys?.let {
            val sealed = SealedInvitationBuilder.buildFromV1(
                sender = it,
                recipient = recipient,
                created = created,
                invitation = invitation
            )
            val peerAddress = recipient.walletAddress

            runBlocking {
                client.publish(
                    envelopes = listOf(
                        EnvelopeBuilder.buildFromTopic(
                            topic = Topic.userInvite(
                                client.address ?: ""
                            ),
                            timestamp = created,
                            message = sealed.toByteArray()
                        ),
                        EnvelopeBuilder.buildFromTopic(
                            topic = Topic.userInvite(
                                peerAddress
                            ),
                            timestamp = created,
                            message = sealed.toByteArray()
                        )
                    )
                )
            }
            return sealed
        }
        return SealedInvitation.newBuilder().build()
    }
}
