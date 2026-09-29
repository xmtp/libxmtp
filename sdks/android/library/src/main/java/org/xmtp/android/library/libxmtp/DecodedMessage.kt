package org.xmtp.android.library.libxmtp

import org.xmtp.android.library.InboxId
import org.xmtp.android.library.Topic
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.toHex
import org.xmtp.proto.message.contents.Content
import uniffi.xmtpv3.FfiContentDecodeFailureKind
import uniffi.xmtpv3.FfiContentTypeId
import uniffi.xmtpv3.FfiConversationMessageKind
import uniffi.xmtpv3.FfiDeliveryCursor
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiUndecodableContent
import java.util.Date

class DecodedMessage private constructor(
    private val libXMTPMessage: FfiMessage,
    private val parsedContent: Content.EncodedContent?,
    private val decodedContent: Any?,
    /** Database-local resume cursor. Present on streamed messages. Reading it does not acknowledge delivery. */
    val deliveryCursor: FfiDeliveryCursor? = null,
    /**
     * Set when the content could not be decoded: the exact received bytes, the
     * received identifier and fallback when present, and the typed cause.
     * `content()` is null for such a message; `fallback` returns the received one.
     */
    val undecodable: FfiUndecodableContent? = null,
) {
    /** The parsed envelope. Throws when the received bytes are not an EncodedContent. */
    val encodedContent: Content.EncodedContent
        get() = parsedContent ?: throw IllegalStateException("the received bytes are not an EncodedContent")

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
        get() = undecodable?.let { it.fallback ?: "" } ?: encodedContent.fallback

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

        /**
         * Null excludes forged membership content. Content that does not parse or
         * decode is kept as an undecodable message with its exact bytes, so history
         * keeps the row and the delivery flow hands it off.
         */
        internal fun createForDelivery(
            libXMTPMessage: FfiMessage,
            deliveryCursor: FfiDeliveryCursor?,
        ): DecodedMessage? {
            fun undecodable(
                parsed: Content.EncodedContent?,
                kind: FfiContentDecodeFailureKind,
                message: String?,
            ): DecodedMessage =
                DecodedMessage(
                    libXMTPMessage,
                    parsed,
                    null,
                    deliveryCursor = deliveryCursor,
                    undecodable =
                        FfiUndecodableContent(
                            rawBytes = libXMTPMessage.content,
                            contentType =
                                parsed?.takeIf { it.hasType() }?.type?.let {
                                    FfiContentTypeId(
                                        authorityId = it.authorityId,
                                        typeId = it.typeId,
                                        versionMajor = it.versionMajor.toUInt(),
                                        versionMinor = it.versionMinor.toUInt(),
                                    )
                                },
                            fallback = parsed?.takeIf { it.hasFallback() }?.fallback,
                            failureKind = kind,
                            failureMessage = message ?: "",
                        ),
                )

            val encodedContent =
                try {
                    EncodedContent.parseFrom(libXMTPMessage.content)
                } catch (e: Exception) {
                    return undecodable(null, FfiContentDecodeFailureKind.MALFORMED_ENVELOPE, e.message)
                }
            if (encodedContent.type == ContentTypeGroupUpdated &&
                libXMTPMessage.kind != FfiConversationMessageKind.MEMBERSHIP_CHANGE
            ) {
                return null
            }
            if (!encodedContent.hasType() ||
                encodedContent.type.authorityId.isEmpty() ||
                encodedContent.type.typeId.isEmpty()
            ) {
                return undecodable(
                    encodedContent,
                    FfiContentDecodeFailureKind.MALFORMED_ENVELOPE,
                    "content type identifier is absent or incomplete",
                )
            }
            val decodedContent =
                try {
                    encodedContent.decoded<Any>()
                } catch (e: Exception) {
                    return undecodable(encodedContent, FfiContentDecodeFailureKind.CODEC_DECODE_FAILED, e.message)
                }
            return DecodedMessage(libXMTPMessage, encodedContent, decodedContent, deliveryCursor = deliveryCursor)
        }
    }
}
