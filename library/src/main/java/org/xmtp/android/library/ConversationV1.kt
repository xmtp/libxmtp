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
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.messages.decrypt
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.sentAt
import org.xmtp.android.library.messages.toPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.android.library.messages.DecryptedMessage
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
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecodedMessage> {
        val pagination =
            Pagination(limit = limit, before = before, after = after, direction = direction)
        val result = runBlocking {
            client.apiClient.envelopes(topic = topic.description, pagination = pagination)
        }

        return result.mapNotNull { envelope ->
            decodeOrNull(envelope = envelope)
        }
    }

    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecryptedMessage> {
        val pagination =
            Pagination(limit = limit, before = before, after = after, direction = direction)

        val envelopes = runBlocking {
            client.apiClient.envelopes(
                topic = Topic.directMessageV1(client.address, peerAddress).description,
                pagination = pagination
            )
        }

        return envelopes.map { decrypt(it) }
    }

    fun decrypt(envelope: Envelope): DecryptedMessage {
        try {
            val message = Message.parseFrom(envelope.message)
            val decrypted = message.v1.decrypt(client.privateKeyBundleV1)

            val encodedMessage = EncodedContent.parseFrom(decrypted)
            val header = message.v1.header

            return DecryptedMessage(
                id = generateId(envelope),
                encodedContent = encodedMessage,
                senderAddress = header.sender.walletAddress,
                sentAt = message.v1.sentAt
            )
        } catch (e: Exception) {
            throw XMTPException("Error decrypting message", e)
        }
    }

    fun decode(envelope: Envelope): DecodedMessage {
        try {
            val decryptedMessage = decrypt(envelope)

            return DecodedMessage(
                id = generateId(envelope),
                client = client,
                topic = envelope.contentTopic,
                encodedContent = decryptedMessage.encodedContent,
                senderAddress = decryptedMessage.senderAddress,
                sent = decryptedMessage.sentAt
            )
        } catch (e: Exception) {
            throw XMTPException("Error decoding message", e)
        }
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
        return send(preparedMessage)
    }

    fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        return send(preparedMessage)
    }

    fun send(encodedContent: EncodedContent, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(encodedContent = encodedContent, options = options)
        return send(preparedMessage)
    }

    fun send(prepared: PreparedMessage): String {
        client.publish(envelopes = prepared.envelopes)
        if (client.contacts.consentList.state(address = peerAddress) == ConsentState.UNKNOWN) {
            client.contacts.allow(addresses = listOf(peerAddress))
        }
        return prepared.messageId
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

        val fallback = codec.fallback(content)
        if (!fallback.isNullOrBlank()) {
            encoded = encoded.toBuilder().also {
                it.fallback = fallback
            }.build()
        }
        val compression = options?.compression
        if (compression != null) {
            encoded = encoded.compress(compression)
        }
        return prepareMessage(encodedContent = encoded, options = options)
    }

    fun prepareMessage(
        encodedContent: EncodedContent,
        options: SendOptions? = null,
    ): PreparedMessage {
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

        val isEphemeral: Boolean = options != null && options.ephemeral

        val env =
            EnvelopeBuilder.buildFromString(
                topic = if (isEphemeral) ephemeralTopic else topic.description,
                timestamp = date,
                message = MessageBuilder.buildFromMessageV1(v1 = message).toByteArray()
            )

        val envelopes = mutableListOf(env)
        if (client.contacts.needsIntroduction(peerAddress) && !isEphemeral) {
            envelopes.addAll(
                listOf(
                    env.toBuilder().apply {
                        contentTopic = Topic.userIntro(peerAddress).description
                    }.build(),
                    env.toBuilder().apply {
                        contentTopic = Topic.userIntro(client.address).description
                    }.build(),
                )
            )
            client.contacts.hasIntroduced[peerAddress] = true
        }
        return PreparedMessage(envelopes)
    }

    private fun generateId(envelope: Envelope): String =
        Hash.sha256(envelope.message.toByteArray()).toHex()

    val ephemeralTopic: String
        get() = topic.description.replace("/xmtp/0/dm-", "/xmtp/0/dmE-")

    fun streamEphemeral(): Flow<Envelope> = flow {
        client.subscribe(topics = listOf(ephemeralTopic)).collect {
            emit(it)
        }
    }

    fun streamDecryptedMessages(): Flow<DecryptedMessage> = flow {
        client.subscribe(listOf(topic.description)).collect {
            emit(decrypt(envelope = it))
        }
    }
}
