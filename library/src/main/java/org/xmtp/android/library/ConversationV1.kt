package org.xmtp.android.library

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.TextCodec
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

    fun send(text: String, options: SendOptions? = null) {
        send(text = text, sendOptions = options, sentAt = null)
    }

    fun send(text: String, sendOptions: SendOptions? = null, sentAt: Date? = null) {
        val encoder = TextCodec()
        val encodedContent = encoder.encode(content = text)
        send(encodedContent = encodedContent, sendOptions = sendOptions, sentAt = sentAt)
    }

    fun <T> send(content: T, options: SendOptions? = null) {
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
        send(encodedContent = encoded, sendOptions = options)
    }

    private fun send(
        encodedContent: EncodedContent,
        sendOptions: SendOptions? = null,
        sentAt: Date? = null,
    ) {
        val contact = client.contacts.find(peerAddress) ?: throw XMTPException("Contact not found.")

        var content = encodedContent

        if (sendOptions?.compression != null) {
            content = content.compress(sendOptions.compression!!)
        }

        val recipient = contact.toPublicKeyBundle()
        if (!recipient.identityKey.hasSignature()) {
            throw XMTPException("no signature for id key")
        }
        val date = sentAt ?: Date()
        if (client.privateKeyBundleV1 == null) {
            throw XMTPException("no private key bundle")
        }
        val message = MessageV1Builder.buildEncode(
            sender = client.privateKeyBundleV1!!,
            recipient = recipient,
            message = content.toByteArray(),
            timestamp = date
        )
        val envelopes = mutableListOf(
            EnvelopeBuilder.buildFromTopic(
                topic = topic,
                timestamp = date,
                message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
            )
        )
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

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        val pagination = Pagination(limit = limit, startTime = before, endTime = after)
        val result = runBlocking {
            client.apiClient.queryTopic(topic = topic, pagination = pagination)
        }

        return result.envelopesList.flatMap { envelope ->
            listOf(decode(envelope = envelope))
        }
    }

    fun decode(envelope: Envelope): DecodedMessage {
        val message = Message.parseFrom(envelope.message)
        val decrypted = message.v1.decrypt(client.privateKeyBundleV1)
        val encodedMessage = EncodedContent.parseFrom(decrypted)
        val header = message.v1.header
        return DecodedMessage(
            encodedContent = encodedMessage,
            senderAddress = header.sender.walletAddress,
            sent = message.v1.sentAt
        )
    }

    fun streamMessages(): Flow<DecodedMessage> = flow {
        client.subscribe(listOf(topic.description)).collect {
            emit(decode(envelope = it))
        }
    }
}
