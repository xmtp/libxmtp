package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Hash
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Message
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV1Builder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.decrypt
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.sentAt
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import java.util.Date

data class ConversationV1(
    val client: Client,
    val peerAddress: String,
    val sentAt: Date,
) {

    val topic: Topic
        get() = Topic.directMessageV1(client.address, peerAddress)

    fun streamMessages(): Flow<DecodedMessage> = flow {
        client.subscribe(listOf(topic.description)).collect {
            emit(decode(envelope = it))
        }
    }

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        val pagination = Pagination(limit = limit, before = before, after = after)
        val result = runBlocking {
            client.apiClient.envelopes(topic = topic.description, pagination = pagination)
        }

        return result.mapNotNull { envelope ->
            decodeOrNull(envelope = envelope)
        }
    }

    fun decode(envelope: Envelope): DecodedMessage {
        val message = Message.parseFrom(envelope.message)
        val decrypted = message.v1.decrypt(client.privateKeyBundleV1)
        val encodedMessage = EncodedContent.parseFrom(decrypted)
        val header = message.v1.header
        val decoded = DecodedMessage(
            encodedContent = encodedMessage,
            senderAddress = header.sender.walletAddress,
            sent = message.v1.sentAt
        )

        decoded.id = generateId(envelope)

        return decoded
    }

    private fun decodeOrNull(envelope: Envelope): DecodedMessage? {
        return try {
            decode(envelope)
        } catch (e: Exception) {
            Log.d("CONV_V1", "discarding message that failed to decode", e)
            null
        }
    }

    fun send(text: String, options: SendOptions? = null): String {
        return send(text = text, sendOptions = options, sentAt = null)
    }

    internal fun send(
        text: String,
        sendOptions: SendOptions? = null,
        sentAt: Date? = null,
    ): String {
        val preparedMessage = prepareMessage(content = text, options = sendOptions)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun send(encodedContent: EncodedContent): String {
        val preparedMessage = prepareMessage(encodedContent = encodedContent)
        preparedMessage.send()
        return preparedMessage.messageId
    }

    fun <T> prepareMessage(content: T, options: SendOptions?): PreparedMessage {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: Any?): EncodedContent {
            val contentType = content as? T
            if (contentType != null) {
                return codec.encode(content = contentType)
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
        return prepareMessage(encodedContent = encoded)
    }

    fun prepareMessage(encodedContent: EncodedContent): PreparedMessage {
        val contact = client.contacts.find(peerAddress) ?: throw XMTPException("address not found")
        val recipient = contact.toPublicKeyBundle()
        if (!recipient.identityKey.hasSignature()) {
            throw Exception("no signature for id key")
        }
        val date = Date()
        val message = MessageV1Builder.buildEncode(
            sender = client.privateKeyBundleV1,
            recipient = recipient,
            message = encodedContent.toByteArray(),
            timestamp = date
        )
        val messageEnvelope =
            EnvelopeBuilder.buildFromTopic(
                topic = Topic.directMessageV1(client.address, peerAddress),
                timestamp = date,
                message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
            )
        return PreparedMessage(
            messageEnvelope = messageEnvelope,
            conversation = Conversation.V1(this)
        ) {
            val envelopes = mutableListOf(messageEnvelope)
            if (client.contacts.needsIntroduction(peerAddress)) {
                envelopes.addAll(
                    listOf(
                        EnvelopeBuilder.buildFromTopic(
                            topic = Topic.userIntro(peerAddress),
                            timestamp = date,
                            message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
                        ),
                        EnvelopeBuilder.buildFromTopic(
                            topic = Topic.userIntro(client.address),
                            timestamp = date,
                            message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
                        )
                    )
                )
                client.contacts.hasIntroduced[peerAddress] = true
            }
            client.publish(envelopes = envelopes)
        }
    }

    private fun generateId(envelope: Envelope): String =
        Hash.sha256(envelope.message.toByteArray()).toHex()
}
