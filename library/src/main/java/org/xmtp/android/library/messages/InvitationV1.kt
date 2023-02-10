package org.xmtp.android.library.messages

import com.google.crypto.tink.subtle.Base64.encodeToString
import com.google.protobuf.kotlin.toByteString
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.InvitationV1.Context
import java.security.SecureRandom

typealias InvitationV1 = org.xmtp.proto.message.contents.Invitation.InvitationV1

class InvitationV1Builder {
    companion object {
        fun buildFromTopic(
            topic: Topic,
            context: Invitation.InvitationV1.Context? = null,
            aes256GcmHkdfSha256: Invitation.InvitationV1.Aes256gcmHkdfsha256
        ): InvitationV1 {
            return InvitationV1.newBuilder().apply {
                this.topic = topic.description
                if (context != null) {
                    this.context = context
                }
                this.aes256GcmHkdfSha256 = aes256GcmHkdfSha256
            }.build()
        }

        fun buildContextFromId(
            conversationId: String = "",
            metadata: Map<String, String> = mapOf()
        ): Invitation.InvitationV1.Context {
            return Invitation.InvitationV1.Context.newBuilder().apply {
                this.conversationId = conversationId
                this.putAllMetadata(metadata)
            }.build()
        }
    }
}

fun InvitationV1.createRandom(context: Invitation.InvitationV1.Context? = null): InvitationV1 {
    val inviteContext = context ?: Invitation.InvitationV1.Context.newBuilder().build()
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
        aes256GcmHkdfSha256 = aes256GcmHkdfSha256
    )
}

class InvitationV1ContextBuilder {
    companion object {
        fun buildFromConversation(
            conversationId: String = "",
            metadata: Map<String, String> = mapOf()
        ): Context {
            return Context.newBuilder().also {
                it.conversationId = conversationId
                it.putAllMetadata(metadata)
            }.build()
        }
    }
}
