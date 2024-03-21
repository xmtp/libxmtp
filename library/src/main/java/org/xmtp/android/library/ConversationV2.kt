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
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Message
import org.xmtp.android.library.messages.MessageBuilder
import org.xmtp.android.library.messages.MessageV2Builder
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.android.library.messages.SealedInvitationHeaderV1
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.walletAddress
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.Invitation
import java.util.Date

data class ConversationV2(
    val topic: String,
    val keyMaterial: ByteArray,
    val context: Invitation.InvitationV1.Context,
    val peerAddress: String,
    val client: Client,
    val createdAtNs: Long? = null,
    private val header: SealedInvitationHeaderV1,
) {

    companion object {
        fun create(
            client: Client,
            invitation: Invitation.InvitationV1,
            header: SealedInvitationHeaderV1,
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
                createdAtNs = header.createdNs,
                header = header,
            )
        }
    }

    val createdAt: Date = Date((createdAtNs ?: header.createdNs) / 1_000_000)

    /**
     * This lists messages sent to the [Conversation].
     * @param before initial date to filter
     * @param after final date to create a range of dates and filter
     * @param limit is the number of result that will be returned
     * @param direction is the way of srting the information, by default is descending, you can
     * know more about it in class [MessageApiOuterClass].
     * @see MessageApiOuterClass.SortDirection
     * @return The list of messages sent. If [before] or [after] are specified then this will only list messages
     * sent at or [after] and at or [before].
     * If [limit] is specified then results are pulled in pages of that size.
     * If [direction] is specified then that will control the sort order of te messages.
     * @see Conversation.messages
     */
    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecodedMessage> {
        val pagination =
            Pagination(limit = limit, before = before, after = after, direction = direction)
        val result = runBlocking {
            client.apiClient.envelopes(
                topic = topic,
                pagination = pagination,
            )
        }

        return result.mapNotNull { envelope ->
            decodeEnvelopeOrNull(envelope)
        }
    }

    /**
     * This lists decrypted messages sent to the [Conversation].
     * @param before initial date to filter
     * @param after final date to create a range of dates and filter
     * @param limit is the number of result that will be returned
     * @param direction is the way of srting the information, by default is descending, you can
     * know more about it in class [MessageApiOuterClass].
     * @see MessageApiOuterClass.SortDirection
     * @return The list of messages sent. If [before] or [after] are specified then this will only list messages
     * sent at or [after] and at or [before].
     * If [limit] is specified then results are pulled in pages of that size.
     * If [direction] is specified then that will control the sort order of te messages.
     */
    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecryptedMessage> {
        val pagination =
            Pagination(limit = limit, before = before, after = after, direction = direction)
        val envelopes = runBlocking { client.apiClient.envelopes(topic, pagination) }

        return envelopes.map { envelope ->
            decrypt(envelope)
        }
    }

    /**
     * This decrypts a message
     * @param envelope Object that contains all the information of the encrypted message
     * @return [DecryptedMessage] object
     */
    fun decrypt(envelope: Envelope): DecryptedMessage {
        val message = Message.parseFrom(envelope.message)
        return MessageV2Builder.buildDecrypt(
            id = generateId(envelope = envelope),
            topic,
            message.v2,
            keyMaterial,
            client,
        )
    }

    fun streamMessages(): Flow<DecodedMessage> = flow {
        client.subscribe(listOf(topic)).mapNotNull { decodeEnvelopeOrNull(envelope = it) }.collect {
            emit(it)
        }
    }

    /**
     * This encrypts a message
     * @param envelope Object that contains all the information of the decrypted message
     * @return [DecodedMessage] object
     */
    fun decodeEnvelope(envelope: Envelope): DecodedMessage {
        val message = Message.parseFrom(envelope.message)
        return MessageV2Builder.buildDecode(
            generateId(envelope = envelope),
            topic = topic,
            message.v2,
            keyMaterial = keyMaterial,
            client = client,
        )
    }

    /**
     * This encrypts a message
     * @param envelope Object that contains all the information of the decrypted message
     * @return [DecodedMessage] object if is not possible will return null
     */
    private fun decodeEnvelopeOrNull(envelope: Envelope): DecodedMessage? {
        return try {
            decodeEnvelope(envelope)
        } catch (e: Exception) {
            Log.d("CONV_V2", "discarding message that failed to decode", e)
            null
        }
    }

    suspend fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        return send(preparedMessage)
    }

    suspend fun send(text: String, options: SendOptions? = null, sentAt: Date? = null): String {
        val preparedMessage = prepareMessage(content = text, options = options)
        return send(preparedMessage)
    }

    suspend fun send(encodedContent: EncodedContent, options: SendOptions?): String {
        val preparedMessage = prepareMessage(encodedContent = encodedContent, options = options)
        return send(preparedMessage)
    }

    suspend fun send(prepared: PreparedMessage): String {
        client.publish(envelopes = prepared.envelopes)
        if (client.contacts.consentList.state(address = peerAddress) == ConsentState.UNKNOWN) {
            client.contacts.allow(addresses = listOf(peerAddress))
        }
        return prepared.messageId
    }

    fun <Codec : ContentCodec<T>, T> encode(codec: Codec, content: T): ByteArray {
        val encodedContent = codec.encode(content = content)
        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = encodedContent,
            topic = topic,
            keyMaterial = keyMaterial,
            codec = codec,
        )
        val envelope = EnvelopeBuilder.buildFromString(
            topic = topic,
            timestamp = Date(),
            message = MessageBuilder.buildFromMessageV2(v2 = message.messageV2).toByteArray(),
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
        return prepareMessage(encoded, options = options)
    }

    fun prepareMessage(
        encodedContent: EncodedContent,
        options: SendOptions?,
    ): PreparedMessage {
        val codec = Client.codecRegistry.find(options?.contentType)
        val message = MessageV2Builder.buildEncode(
            client = client,
            encodedContent = encodedContent,
            topic = topic,
            keyMaterial = keyMaterial,
            codec = codec,
        )

        val newTopic = if (options?.ephemeral == true) ephemeralTopic else topic

        val envelope = EnvelopeBuilder.buildFromString(
            topic = newTopic,
            timestamp = Date(),
            message = MessageBuilder.buildFromMessageV2(v2 = message.messageV2).toByteArray(),
        )
        return PreparedMessage(listOf(envelope))
    }

    private fun generateId(envelope: Envelope): String =
        Hash.sha256(envelope.message.toByteArray()).toHex()

    val ephemeralTopic: String
        get() = topic.replace("/xmtp/0/m", "/xmtp/0/mE")

    fun streamEphemeral(): Flow<Envelope> = flow {
        client.subscribe(listOf(ephemeralTopic)).collect {
            emit(it)
        }
    }

    fun streamDecryptedMessages(): Flow<DecryptedMessage> = flow {
        client.subscribe(listOf(topic)).collect {
            emit(decrypt(envelope = it))
        }
    }
}
