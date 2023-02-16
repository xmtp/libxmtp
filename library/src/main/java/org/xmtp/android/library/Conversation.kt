package org.xmtp.android.library

import org.xmtp.android.library.messages.Envelope
import java.util.Date

sealed class Conversation {
    data class V1(val conversationV1: ConversationV1) : Conversation()
    data class V2(val conversationV2: ConversationV2) : Conversation()

    val createdAt: Date
        get() {
            return when (this) {
                is V1 -> conversationV1.sentAt
                is V2 -> conversationV2.createdAt
            }
        }

    val peerAddress: String
        get() {
            return when (this) {
                is V1 -> conversationV1.peerAddress
                is V2 -> conversationV2.peerAddress
            }
        }

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

    fun decode(envelope: Envelope): DecodedMessage {
        when (this) {
            is V1 -> return conversationV1.decode(envelope)
            is V2 -> return conversationV2.decodeEnvelope(envelope)
        }
    }

    fun <T> send(content: T, options: SendOptions? = null) {
        when (this) {
            is V1 -> conversationV1.send(content = content, options = options)
            is V2 -> conversationV2.send(content = content, options = options)
        }
    }

    fun send(text: String, sendOptions: SendOptions? = null, sentAt: Date? = null) {
        when (this) {
            is V1 -> conversationV1.send(text = text, sendOptions, sentAt)
            is V2 -> conversationV2.send(text = text, sendOptions, sentAt)
        }
    }

    val topic: String
        get() {
            return when (this) {
                is V1 -> conversationV1.topic.description
                is V2 -> conversationV2.topic
            }
        }

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
    ): List<DecodedMessage> {
        return when (this) {
            is V1 -> conversationV1.messages(
                limit = limit,
                before = before,
                after = after
            )
            is V2 ->
                conversationV2.messages(
                    limit = limit,
                    before = before,
                    after = after
                )
        }
    }

    val client: Client
        get() {
            return when (this) {
                is V1 -> conversationV1.client
                is V2 -> conversationV2.client
            }
        }
}
