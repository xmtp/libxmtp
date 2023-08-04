package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Hash
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
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
    val topic: String,
    val keyMaterial: ByteArray,
    val context: Invitation.InvitationV1.Context,
    val peerAddress: String,
    val client: Client,
    val isGroup: Boolean = false,
    private val header: SealedInvitationHeaderV1,
) {

    companion object {
        fun create(
            client: Client,
            invitation: Invitation.InvitationV1,
            header: SealedInvitationHeaderV1,
            isGroup: Boolean = false,
        ): ConversationV2 {
            val myKeys = client.keys.getPublicKeyBundle()
            val peer =
                if (myKeys.walletAddress == (header.sender.walletAddress)) header.recipient else header.sender
            val peerAddress = peer.walletAddress
            val keyMaterial = invitation.aes256GcmHkdfSha256.keyMaterial.toByteArray()
            return ConversationV2(
                topic = invitation.topic,
                keyMaterial = keyMaterial,
                context = invitation.context,
                peerAddress = peerAddress,
                client = client,
                header = header,
                isGroup = isGroup
            )
        }
    }

    val createdAt: Date = Date(header.createdNs / 1_000_000)

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        val pagination = Pagination(limit = limit, before = before, after = after)
        val result = runBlocking {
            client.apiClient.envelopes(
                topic = topic,
                pagination = pagination
            )
        }

        return result.mapNotNull { envelope ->
            decodeEnvelopeOrNull(envelope)
        }
    }

    fun streamMessages(): Flow<DecodedMessage> = flow {
        client.subscribe(listOf(topic)).mapNotNull { decodeEnvelopeOrNull(envelope = it) }.collect {
            emit(it)
        }
    }

    fun decodeEnvelope(envelope: Envelope): DecodedMessage {
        val message = Message.parseFrom(envelope.message)
        val decoded = decode(message.v2)
        decoded.id = generateId(envelope)
        return decoded
    }

    private fun decodeEnvelopeOrNull(envelope: Envelope): DecodedMessage? {
        return try {
            decodeEnvelope(envelope)
        } catch (e: Exception) {
            Log.d("CONV_V2", "discarding message that failed to decode", e)
            null
        }
    }

    fun decode(message: MessageV2): DecodedMessage =
        MessageV2Builder.buildDecode(message, keyMaterial = keyMaterial, topic = topic)

    fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun send(text: String, options: SendOptions? = null, sentAt: Date? = null): String {
        val preparedMessage = prepareMessage(content = text, options = options)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun send(encodedContent: EncodedContent, options: SendOptions?): String {
        val preparedMessage = prepareMessage(encodedContent = encodedContent, options = options)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun <Codec : ContentCodec<T>, T> encode(codec: Codec, content: T): ByteArray {
        val encodedContent = codec.encode(content = content)
        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = encodedContent,
            topic = topic,
            keyMaterial = keyMaterial
        )
        val envelope = EnvelopeBuilder.buildFromString(
            topic = topic,
            timestamp = Date(),
            message = MessageBuilder.buildFromMessageV2(v2 = message).toByteArray()
        )
        return envelope.toByteArray()
    }

    fun <T> prepareMessage(content: T, options: SendOptions?): PreparedMessage {
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
        val compression = options?.compression
        if (compression != null) {
            encoded = encoded.compress(compression)
        }
        return prepareMessage(encoded, options = options)
    }

    fun prepareMessage(encodedContent: EncodedContent, options: SendOptions?): PreparedMessage {
        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = encodedContent,
            topic = topic,
            keyMaterial = keyMaterial
        )

        val newTopic = if (options?.ephemeral == true) ephemeralTopic else topic

        val envelope = EnvelopeBuilder.buildFromString(
            topic = newTopic,
            timestamp = Date(),
            message = MessageBuilder.buildFromMessageV2(v2 = message).toByteArray()
        )
        return PreparedMessage(messageEnvelope = envelope, conversation = Conversation.V2(this)) {
            client.publish(envelopes = listOf(envelope))
        }
    }

    private fun generateId(envelope: Envelope): String =
        Hash.sha256(envelope.message.toByteArray()).toHex()

    val ephemeralTopic: String
        get() = topic.replace("/xmtp/0/m", "/xmtp/0/mE")

    fun streamEphemeral(): Flow<Envelope> = flow {
        client.subscribe(topics = listOf(ephemeralTopic)).collect {
            emit(it)
        }
    }
}
