package org.xmtp.android.library.libxmtp

import kotlinx.coroutines.CancellationException
import org.xmtp.android.library.InboxId
import org.xmtp.android.library.Topic
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.Content
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiDeliveryCursor
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiMessage
import java.util.Date

class DecodedMessage private constructor(
    private val libXMTPMessage: FfiMessage,
    private val parsedContent: Content.EncodedContent?,
    private val decodedContent: Any?,
    /** Database-local resume cursor. Present on streamed messages. Reading it does not acknowledge delivery. */
    val deliveryCursor: FfiDeliveryCursor? = null,
) {
    val encodedContent: Content.EncodedContent
        get() = parsedContent ?: EncodedContent.parseFrom(libXMTPMessage.content)

    enum class MessageDeliveryStatus {
        ALL,
        PUBLISHED,
        UNPUBLISHED,
        FAILED,
    }

    enum class SortDirection {
        ASCENDING,
        DESCENDING,
    }

    enum class SortBy {
        SENT_TIME,
        INSERTED_TIME,
    }

    val id: String
        get() = libXMTPMessage.id.toHex()

    val conversationId: String
        get() = libXMTPMessage.conversationId.toHex()

    val senderInboxId: InboxId
        get() = libXMTPMessage.senderInboxId

    val kind: FfiConversationMessageKind
        get() = libXMTPMessage.kind

    val sentAt: Date
        get() = Date(libXMTPMessage.sentAtNs / 1_000_000)

    val sentAtNs: Long
        get() = libXMTPMessage.sentAtNs

    val insertedAtNs: Long
        get() = libXMTPMessage.insertedAtNs

    val expiresAtNs: Long?
        get() = libXMTPMessage.expireAtNs

    val expiresAt: Date?
        get() = expiresAtNs?.let { Date(it / 1_000_000) }

    val deliveryStatus: MessageDeliveryStatus
        get() =
            when (libXMTPMessage.deliveryStatus) {
                FfiDeliveryStatus.UNPUBLISHED -> MessageDeliveryStatus.UNPUBLISHED
                FfiDeliveryStatus.PUBLISHED -> MessageDeliveryStatus.PUBLISHED
                FfiDeliveryStatus.FAILED -> MessageDeliveryStatus.FAILED
            }

    val topic: String
        get() = Topic.groupMessage(conversationId).description

    @Suppress("UNCHECKED_CAST")
    fun <T> content(): T? = decodedContent as? T

    val fallback: String
        get() = parsedContent?.fallback.orEmpty()

    val body: String
        get() {
            return content() as? String ?: fallback
        }

    companion object {
        fun create(libXMTPMessage: FfiMessage): DecodedMessage? = create(libXMTPMessage, null)

        fun create(
            libXMTPMessage: FfiMessage,
            deliveryCursor: FfiDeliveryCursor?,
        ): DecodedMessage? = createForDelivery(libXMTPMessage, deliveryCursor)

        private inline fun <T> decodeOrNull(decode: () -> T): T? =
            try {
                decode()
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                null
            }

        /** Keep decode failures for handoff. Null excludes forged membership content. */
        internal fun createForDelivery(
            libXMTPMessage: FfiMessage,
            deliveryCursor: FfiDeliveryCursor?,
        ): DecodedMessage? {
            val encodedContent = decodeOrNull { EncodedContent.parseFrom(libXMTPMessage.content) }
            if (encodedContent?.type == ContentTypeGroupUpdated &&
                libXMTPMessage.kind != FfiConversationMessageKind.MEMBERSHIP_CHANGE
            ) {
                return null
            }
            val decodedContent = decodeOrNull { encodedContent?.decoded<Any>() }
            return DecodedMessage(libXMTPMessage, encodedContent, decodedContent, deliveryCursor = deliveryCursor)
        }
    }
}
