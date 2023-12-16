package org.xmtp.android.library

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.flow.Flow
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.messages.Envelope
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.proto.keystore.api.v1.Keystore.TopicMap.TopicData
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.contents.Invitation
import org.xmtp.proto.message.contents.Invitation.InvitationV1.Aes256gcmHkdfsha256
import org.xmtp.android.library.messages.DecryptedMessage
import java.util.Date

/**
 * This represents an ongoing conversation.
 * It can be provided to [Client] to [messages] and [send].
 * The [Client] also allows you to [streamMessages] from this [Conversation].
 *
 * It attempts to give uniform shape to v1 and v2 conversations.
 */
sealed class Conversation {
    data class V1(val conversationV1: ConversationV1) : Conversation()
    data class V2(val conversationV2: ConversationV2) : Conversation()

    enum class Version { V1, V2 }

    // This indicates whether this a v1 or v2 conversation.
    val version: Version
        get() {
            return when (this) {
                is V1 -> Version.V1
                is V2 -> Version.V2
            }
        }

    // When the conversation was first created.
    val createdAt: Date
        get() {
            return when (this) {
                is V1 -> conversationV1.sentAt
                is V2 -> conversationV2.createdAt
            }
        }

    // This is the address of the peer that I am talking to.
    val peerAddress: String
        get() {
            return when (this) {
                is V1 -> conversationV1.peerAddress
                is V2 -> conversationV2.peerAddress
            }
        }

    // This distinctly identifies between two addresses.
    // Note: this will be empty for older v1 conversations.
    val conversationId: String?
        get() {
            return when (this) {
                is V1 -> null
                is V2 -> conversationV2.context.conversationId
            }
        }

    val keyMaterial: ByteArray?
        get() {
            return when (this) {
                is V1 -> null
                is V2 -> conversationV2.keyMaterial
            }
        }

    fun consentState(): ConsentState {
        val client: Client = when (this) {
            is V1 -> conversationV1.client
            is V2 -> conversationV2.client
        }
        return client.contacts.consentList.state(address = peerAddress)
    }

    /**
     * This method is to create a TopicData object
     * @return [TopicData] that contains all the information about the Topic, the conversation
     * context and the necessary encryption data for it.
     */
    fun toTopicData(): TopicData {
        val data = TopicData.newBuilder()
            .setCreatedNs(createdAt.time * 1_000_000)
            .setPeerAddress(peerAddress)
        return when (this) {
            is V1 -> data.build()
            is V2 -> data.setInvitation(
                Invitation.InvitationV1.newBuilder()
                    .setTopic(topic)
                    .setContext(conversationV2.context)
                    .setAes256GcmHkdfSha256(
                        Aes256gcmHkdfsha256.newBuilder()
                            .setKeyMaterial(conversationV2.keyMaterial.toByteString()),
                    ),
            ).build()
        }
    }

    fun decode(envelope: Envelope): DecodedMessage {
        return when (this) {
            is V1 -> conversationV1.decode(envelope)
            is V2 -> conversationV2.decodeEnvelope(envelope)
        }
    }

    fun decodeOrNull(envelope: Envelope): DecodedMessage? {
        return try {
            decode(envelope)
        } catch (e: Exception) {
            Log.d("CONVERSATION", "discarding message that failed to decode", e)
            null
        }
    }

    fun <T> prepareMessage(content: T, options: SendOptions? = null): PreparedMessage {
        return when (this) {
            is V1 -> {
                conversationV1.prepareMessage(content = content, options = options)
            }

            is V2 -> {
                conversationV2.prepareMessage(content = content, options = options)
            }
        }
    }

    fun send(prepared: PreparedMessage): String {
        return when (this) {
            is V1 -> conversationV1.send(prepared = prepared)
            is V2 -> conversationV2.send(prepared = prepared)
        }
    }

    fun <T> send(content: T, options: SendOptions? = null): String {
        return when (this) {
            is V1 -> conversationV1.send(content = content, options = options)
            is V2 -> conversationV2.send(content = content, options = options)
        }
    }

    fun send(text: String, sendOptions: SendOptions? = null, sentAt: Date? = null): String {
        return when (this) {
            is V1 -> conversationV1.send(text = text, sendOptions, sentAt)
            is V2 -> conversationV2.send(text = text, sendOptions, sentAt)
        }
    }

    fun send(encodedContent: EncodedContent, options: SendOptions? = null): String {
        return when (this) {
            is V1 -> conversationV1.send(encodedContent = encodedContent, options = options)
            is V2 -> conversationV2.send(encodedContent = encodedContent, options = options)
        }
    }

    val clientAddress: String
        get() {
            return client.address
        }

    // Is the topic of the conversation depending of the version
    val topic: String
        get() {
            return when (this) {
                is V1 -> conversationV1.topic.description
                is V2 -> conversationV2.topic
            }
        }

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
     */
    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecodedMessage> {
        return when (this) {
            is V1 -> conversationV1.messages(
                limit = limit,
                before = before,
                after = after,
                direction = direction,
            )

            is V2 ->
                conversationV2.messages(
                    limit = limit,
                    before = before,
                    after = after,
                    direction = direction,
                )
        }
    }

    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
    ): List<DecryptedMessage> {
        return when (this) {
            is V1 -> conversationV1.decryptedMessages(limit, before, after, direction)
            is V2 -> conversationV2.decryptedMessages(limit, before, after, direction)
        }
    }

    fun decrypt(
        envelope: Envelope,
    ): DecryptedMessage {
        return when (this) {
            is V1 -> conversationV1.decrypt(envelope)
            is V2 -> conversationV2.decrypt(envelope)
        }
    }

    // Get the client according to the version
    val client: Client
        get() {
            return when (this) {
                is V1 -> conversationV1.client
                is V2 -> conversationV2.client
            }
        }

    /**
     * This exposes a stream of new messages sent to the [Conversation].
     * @return Stream of messages according to the version
     */
    fun streamMessages(): Flow<DecodedMessage> {
        return when (this) {
            is V1 -> conversationV1.streamMessages()
            is V2 -> conversationV2.streamMessages()
        }
    }

    fun streamDecryptedMessages(): Flow<DecryptedMessage> {
        return when (this) {
            is V1 -> conversationV1.streamDecryptedMessages()
            is V2 -> conversationV2.streamDecryptedMessages()
        }
    }

    fun streamEphemeral(): Flow<Envelope> {
        return when (this) {
            is V1 -> return conversationV1.streamEphemeral()
            is V2 -> return conversationV2.streamEphemeral()
        }
    }
}
