package org.xmtp.android.library.libxmtp

import android.util.Log
import org.xmtp.android.library.Client
import org.xmtp.android.library.DecodedMessage
import org.xmtp.android.library.XMTPException
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.Topic
import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiGroupMessageKind
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.org.xmtp.android.library.codecs.ContentTypeGroupMembershipChange
import java.util.Date

data class MessageV3(val client: Client, private val libXMTPMessage: FfiMessage) {

    val id: ByteArray
        get() = libXMTPMessage.id

    val convoId: ByteArray
        get() = libXMTPMessage.convoId

    val senderAddress: String
        get() = libXMTPMessage.addrFrom

    val sentAt: Date
        get() = Date(libXMTPMessage.sentAtNs / 1_000_000)

    val deliveryStatus: MessageDeliveryStatus
        get() = when (libXMTPMessage.deliveryStatus) {
            FfiDeliveryStatus.UNPUBLISHED -> MessageDeliveryStatus.UNPUBLISHED
            FfiDeliveryStatus.PUBLISHED -> MessageDeliveryStatus.PUBLISHED
            FfiDeliveryStatus.FAILED -> MessageDeliveryStatus.FAILED
        }

    fun decode(): DecodedMessage {
        try {
            val decodedMessage = DecodedMessage(
                id = id.toHex(),
                client = client,
                topic = Topic.groupMessage(convoId.toHex()).description,
                encodedContent = EncodedContent.parseFrom(libXMTPMessage.content),
                senderAddress = senderAddress,
                sent = sentAt,
                deliveryStatus = deliveryStatus
            )
            if (decodedMessage.encodedContent.type == ContentTypeGroupMembershipChange && libXMTPMessage.kind != FfiGroupMessageKind.MEMBERSHIP_CHANGE) {
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

    fun decryptOrNull(): DecryptedMessage? {
        return try {
            decrypt()
        } catch (e: Exception) {
            Log.d("MESSAGE_V3", "discarding message that failed to decrypt", e)
            null
        }
    }

    fun decrypt(): DecryptedMessage {
        return DecryptedMessage(
            id = id.toHex(),
            topic = Topic.groupMessage(convoId.toHex()).description,
            encodedContent = decode().encodedContent,
            senderAddress = senderAddress,
            sentAt = Date(),
            deliveryStatus = deliveryStatus
        )
    }
}
