package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.Content
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageWithReactions
import java.util.Date

class Message private constructor(
    private val libXMTPMessage: FfiMessage,
    val encodedContent: Content.EncodedContent,
    private val decodedContent: Any?,
    val childMessages: List<Message>? = null
) {
    enum class MessageDeliveryStatus {
        ALL, PUBLISHED, UNPUBLISHED, FAILED
    }

    enum class SortDirection {
        ASCENDING,
        DESCENDING;
    }

    val id: String
        get() = libXMTPMessage.id.toHex()

    val convoId: String
        get() = libXMTPMessage.convoId.toHex()

    val senderInboxId: String
        get() = libXMTPMessage.senderInboxId

    val sentAt: Date
        get() = Date(libXMTPMessage.sentAtNs / 1_000_000)

    val sentAtNs: Long
        get() = libXMTPMessage.sentAtNs

    val deliveryStatus: MessageDeliveryStatus
        get() = when (libXMTPMessage.deliveryStatus) {
            FfiDeliveryStatus.UNPUBLISHED -> MessageDeliveryStatus.UNPUBLISHED
            FfiDeliveryStatus.PUBLISHED -> MessageDeliveryStatus.PUBLISHED
            FfiDeliveryStatus.FAILED -> MessageDeliveryStatus.FAILED
        }

    val topic: String
        get() = Topic.groupMessage(convoId).description

    @Suppress("UNCHECKED_CAST")
    fun <T> content(): T? = decodedContent as? T

    val fallbackContent: String
        get() = encodedContent.fallback

    val body: String
        get() {
            return content() as? String ?: fallbackContent
        }

    companion object {
        fun create(libXMTPMessage: FfiMessage): Message? {
            return try {
                val encodedContent = EncodedContent.parseFrom(libXMTPMessage.content)
                if (encodedContent.type == ContentTypeGroupUpdated && libXMTPMessage.kind != FfiConversationMessageKind.MEMBERSHIP_CHANGE) {
                    throw XMTPException("Error decoding group membership change")
                }
                // Decode the content once during creation
                val decodedContent = encodedContent.decoded<Any>()
                Message(libXMTPMessage, encodedContent, decodedContent)
            } catch (e: Exception) {
                null // Return null if decoding fails
            }
        }

        fun create(libXMTPMessageWithReactions: FfiMessageWithReactions): Message? {
            return try {
                val encodedContent = EncodedContent.parseFrom(libXMTPMessageWithReactions.message.content)
                if (encodedContent.type == ContentTypeGroupUpdated && libXMTPMessageWithReactions.message.kind != FfiConversationMessageKind.MEMBERSHIP_CHANGE) {
                    throw XMTPException("Error decoding group membership change")
                }
                // Decode the content once during creation
                val decodedContent = encodedContent.decoded<Any>()

                // Convert reactions to Message objects
                val reactionMessages = libXMTPMessageWithReactions.reactions.mapNotNull { create(it) }

                Message(
                    libXMTPMessageWithReactions.message,
                    encodedContent,
                    decodedContent,
                    reactionMessages
                )
            } catch (e: Exception) {
                null // Return null if decoding fails
            }
        }
    }
}
