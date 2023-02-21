package org.xmtp.android.library

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Hash
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Message
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV2
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.contents.Invitation
import java.util.Date

data class ConversationV2(
    var topic: String,
    var keyMaterial: ByteArray,
    var context: Invitation.InvitationV1.Context,
    var peerAddress: String,
    var client: Client,
    private var header: SealedInvitationHeaderV1,
) {
    companion object {

        fun create(
            client: Client,
            invitation: Invitation.InvitationV1,
            header: SealedInvitationHeaderV1,
        ): ConversationV2 {
            val myKeys = client.keys?.getPublicKeyBundle()
            val peer =
                if (myKeys?.walletAddress == (header.sender.walletAddress)) header.recipient else header.sender
            val peerAddress = peer.walletAddress
            val keyMaterial = invitation.aes256GcmHkdfSha256.keyMaterial.toByteArray()
            return ConversationV2(
                topic = invitation.topic,
                keyMaterial = keyMaterial,
                context = invitation.context,
                peerAddress = peerAddress,
                client = client,
                header = header
            )
        }
    }

    val createdAt: Date = Date(header.createdNs / 1_000_000)

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        val pagination = Pagination(limit = limit, startTime = before, endTime = after)
        val result = runBlocking {
            client.apiClient.query(
                topics = listOf(topic),
                pagination = pagination,
                cursor = null
            )
        }

        return result.envelopesList.flatMap { envelope ->
            listOf(decodeEnvelope(envelope))
        }
    }

    fun decodeEnvelope(envelope: Envelope): DecodedMessage {
        val message = Message.parseFrom(envelope.message)
        val decoded = decode(message.v2)
        decoded.id = generateID(envelope)
        return decoded
    }

    fun decode(message: MessageV2): DecodedMessage =
        MessageV2Builder.buildDecode(message, keyMaterial = keyMaterial)

    fun <T> send(content: T, options: SendOptions? = null) {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: Any?): EncodedContent {
            val contentType = content as? T
            if (contentType != null) {
                return codec.encode(contentType)
            } else {
                throw XMTPException("Codec type is not registered")
            }
        }

        var encoded = encode(codec = codec as ContentCodec<T>, content = content)
        encoded = encoded.toBuilder().also {
            it.fallback = options?.contentFallback ?: ""
        }.build()
        send(encodedContent = encoded, sentAt = Date())
    }

    fun send(text: String, options: SendOptions? = null, sentAt: Date? = null) {
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = text)
        send(encodedContent = encodedContent, options = options, sentAt = sentAt)
    }

    private fun send(
        encodedContent: EncodedContent,
        options: SendOptions? = null,
        sentAt: Date? = null,
    ) {
        if (client.getUserContact(peerAddress = peerAddress) == null) {
            throw XMTPException("Contact not found.")
        }
        var content = encodedContent
        val date = sentAt ?: Date()

        if (options?.compression != null) {
            content = content.compress(options.compression!!)
        }

        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = content,
            topic = topic,
            keyMaterial = keyMaterial
        )
        client.publish(
            envelopes = listOf(
                EnvelopeBuilder.buildFromString(
                    topic = topic,
                    timestamp = date,
                    message = MessageBuilder.buildFromMessageV2(message).toByteArray()
                )
            )
        )
    }

    fun streamMessages(): Flow<DecodedMessage> = flow {
        client.subscribe(listOf(topic)).collect {
            emit(decodeEnvelope(envelope = it))
        }
    }

    private fun generateID(envelope: Envelope): String =
        Hash.sha256(envelope.message.toByteArray()).toHex()
}
