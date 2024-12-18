package org.xmtp.android.library.libxmtp

import android.util.Log
import org.xmtp.android.library.Client
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiMessage
import java.util.Date

data class Message(val client: Client, private val libXMTPMessage: FfiMessage) {
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

    fun decode(): DecodedMessage {
        try {
            val decodedMessage = DecodedMessage(
                id = id,
                client = client,
                topic = Topic.groupMessage(convoId).description,
                encodedContent = EncodedContent.parseFrom(libXMTPMessage.content),
                senderInboxId = senderInboxId,
                sent = sentAt,
                sentNs = sentAtNs,
                deliveryStatus = deliveryStatus
            )
            if (decodedMessage.encodedContent.type == ContentTypeGroupUpdated && libXMTPMessage.kind != FfiConversationMessageKind.MEMBERSHIP_CHANGE) {
                throw XMTPException("Error decoding group membership change")
            }
            return decodedMessage
        } catch (e: Exception) {
            throw XMTPException("Error decoding message", e)
        }
    }

    fun decodeOrNull(): DecodedMessage? {
        return try {
            decode()
        } catch (e: Exception) {
            Log.d("MESSAGE_V3", "discarding message that failed to decode", e)
            null
        }
    }
}
