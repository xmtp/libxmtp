package org.xmtp.android.library.messages

import com.google.crypto.tink.subtle.Base64.encodeToString
import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.Crypto
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.ConsentProofPayload
import org.xmtp.proto.message.contents.Invitation.InvitationV1.Context
import java.security.SecureRandom

typealias InvitationV1 = org.xmtp.proto.message.contents.Invitation.InvitationV1

class InvitationV1Builder {
    companion object {
        fun buildFromTopic(
            topic: Topic,
            context: Context? = null,
            aes256GcmHkdfSha256: Invitation.InvitationV1.Aes256gcmHkdfsha256,
            consentProof: ConsentProofPayload? = null
        ): InvitationV1 {
            return InvitationV1.newBuilder().apply {
                this.topic = topic.description
                if (context != null) {
                    this.context = context
                }
                if (consentProof != null) {
                    this.consentProof = consentProof
                }
                this.aes256GcmHkdfSha256 = aes256GcmHkdfSha256
            }.build()
        }

        fun buildContextFromId(
            conversationId: String = "",
            metadata: Map<String, String> = mapOf(),
        ): Context {
            return Context.newBuilder().apply {
                this.conversationId = conversationId
                this.putAllMetadata(metadata)
            }.build()
        }
    }
}

fun InvitationV1.createRandom(context: Context? = null): InvitationV1 {
    val inviteContext = context ?: Context.newBuilder().build()
    val randomBytes = SecureRandom().generateSeed(32)
    val randomString = encodeToString(randomBytes, 0).replace(Regex("=*$"), "")
        .replace(Regex("[^A-Za-z0-9]"), "")
    val topic = Topic.directMessageV2(randomString)
    val keyMaterial = SecureRandom().generateSeed(32)
    val aes256GcmHkdfSha256 = Invitation.InvitationV1.Aes256gcmHkdfsha256.newBuilder().apply {
        this.keyMaterial = keyMaterial.toByteString()
    }.build()

    return InvitationV1Builder.buildFromTopic(
        topic = topic,
        context = inviteContext,
        aes256GcmHkdfSha256 = aes256GcmHkdfSha256,
    )
}

fun InvitationV1.createDeterministic(
    sender: PrivateKeyBundleV2,
    recipient: SignedPublicKeyBundle,
    context: Context? = null,
    consentProof: ConsentProofPayload? = null
): InvitationV1 {
    val myAddress = sender.toV1().walletAddress
    val theirAddress = recipient.walletAddress

    val inviteContext = context ?: Context.newBuilder().build()
    val secret = sender.sharedSecret(
        peer = recipient,
        myPreKey = sender.preKeysList[0].publicKey,
        isRecipient = myAddress < theirAddress,
    )

    val addresses = arrayOf(myAddress, theirAddress)
    addresses.sort()

    val msg = if (context != null && !context.conversationId.isNullOrBlank()) {
        context.conversationId + addresses.joinToString(separator = ",")
    } else {
        addresses.joinToString(separator = ",")
    }

    val topicId = Crypto.calculateMac(secret = secret, message = msg.toByteArray()).toHex()
    val topic = Topic.directMessageV2(topicId)
    val keyMaterial = Crypto.deriveKey(
        secret = secret,
        salt = "__XMTP__INVITATION__SALT__XMTP__".toByteArray(),
        info = listOf("0").plus(addresses).joinToString(separator = "|").toByteArray(),
    )
    val aes256GcmHkdfSha256 = Invitation.InvitationV1.Aes256gcmHkdfsha256.newBuilder().apply {
        this.keyMaterial = keyMaterial.toByteString()
    }.build()

    return InvitationV1Builder.buildFromTopic(
        topic = topic,
        context = inviteContext,
        aes256GcmHkdfSha256 = aes256GcmHkdfSha256,
        consentProof = consentProof
    )
}

class InvitationV1ContextBuilder {
    companion object {
        fun buildFromConversation(
            conversationId: String = "",
            metadata: Map<String, String> = mapOf(),
        ): Context {
            return Context.newBuilder().also {
                it.conversationId = conversationId
                it.putAllMetadata(metadata)
            }.build()
        }
    }
}
