package org.xmtp.android.library.libxmtp

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import org.xmtp.android.library.InboxId
import org.xmtp.android.library.codecs.Attachment
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.MultiRemoteAttachment
import org.xmtp.android.library.codecs.Reaction
import org.xmtp.android.library.codecs.ReactionAction
import org.xmtp.android.library.codecs.ReactionSchema
import org.xmtp.android.library.codecs.ReadReceipt
import org.xmtp.android.library.codecs.RemoteAttachment
import org.xmtp.android.library.codecs.RemoteAttachmentInfo
import org.xmtp.android.library.codecs.TransactionReference
import org.xmtp.android.library.codecs.decoded
import org.xmtp.android.library.codecs.encodedContentFromFfi
import org.xmtp.android.library.toHex
import uniffi.xmtpv3.FfiAttachment
import uniffi.xmtpv3.FfiDecodedMessage
import uniffi.xmtpv3.FfiDecodedMessageBody
import uniffi.xmtpv3.FfiDecodedMessageContent
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiGroupUpdated
import uniffi.xmtpv3.FfiInbox
import uniffi.xmtpv3.FfiMetadataFieldChange
import uniffi.xmtpv3.FfiMultiRemoteAttachment
import uniffi.xmtpv3.FfiReactionAction
import uniffi.xmtpv3.FfiReactionPayload
import uniffi.xmtpv3.FfiReactionSchema
import uniffi.xmtpv3.FfiRemoteAttachment
import uniffi.xmtpv3.FfiRemoteAttachmentInfo
import uniffi.xmtpv3.FfiTransactionMetadata
import uniffi.xmtpv3.FfiTransactionReference
import java.net.URL
import java.util.Date

class DecodedMessageV2 private constructor(
    private val libXMTPMessage: FfiDecodedMessage,
) {
    val id: String
        get() = libXMTPMessage.id().toHex()

    val conversationId: String
        get() = libXMTPMessage.conversationId().toHex()

    val senderInboxId: InboxId
        get() = libXMTPMessage.senderInboxId()

    val sentAt: Date
        get() = Date(libXMTPMessage.sentAtNs() / 1_000_000)

    val sentAtNs: Long
        get() = libXMTPMessage.sentAtNs()

    val insertedAtNs: Long
        get() = libXMTPMessage.insertedAtNs()

    val deliveryStatus: DecodedMessage.MessageDeliveryStatus
        get() =
            when (libXMTPMessage.deliveryStatus()) {
                FfiDeliveryStatus.UNPUBLISHED -> DecodedMessage.MessageDeliveryStatus.UNPUBLISHED
                FfiDeliveryStatus.PUBLISHED -> DecodedMessage.MessageDeliveryStatus.PUBLISHED
                FfiDeliveryStatus.FAILED -> DecodedMessage.MessageDeliveryStatus.FAILED
            }

    val reactions: List<DecodedMessageV2>
        get() = libXMTPMessage.reactions().mapNotNull { create(it) }

    val hasReactions: Boolean
        get() = libXMTPMessage.hasReactions()

    val reactionCount: ULong
        get() = libXMTPMessage.reactionCount()

    val fallbackText: String?
        get() = libXMTPMessage.fallbackText()

    val contentTypeId: ContentTypeId
        get() = ContentTypeIdBuilder.fromFfi(libXMTPMessage.contentTypeId())

    @Suppress("UNCHECKED_CAST")
    fun <T> content(): T? =
        try {
            decodeContent(libXMTPMessage.content()) as? T
        } catch (e: Exception) {
            Log.e("DecodedMessageV2", "Error decoding content: ${e.message}")
            null
        }

    companion object {
        fun create(libXMTPMessage: FfiDecodedMessage): DecodedMessageV2? =
            try {
                DecodedMessageV2(libXMTPMessage)
            } catch (e: Exception) {
                Log.e("DecodedMessageV2", "Error creating DecodedMessageV2: ${e.message}")
                null
            }

        // Helper functions for mapping FFI types to domain types

        private fun mapReaction(ffiReaction: FfiReactionPayload): Reaction {
            val action =
                when (ffiReaction.action) {
                    FfiReactionAction.ADDED -> ReactionAction.Added
                    FfiReactionAction.REMOVED -> ReactionAction.Removed
                    FfiReactionAction.UNKNOWN -> ReactionAction.Unknown
                }
            val schema =
                when (ffiReaction.schema) {
                    FfiReactionSchema.UNICODE -> ReactionSchema.Unicode
                    FfiReactionSchema.SHORTCODE -> ReactionSchema.Shortcode
                    FfiReactionSchema.CUSTOM -> ReactionSchema.Custom
                    FfiReactionSchema.UNKNOWN -> ReactionSchema.Unknown
                }
            return Reaction(
                reference = ffiReaction.reference,
                action = action,
                content = ffiReaction.content,
                schema = schema,
            )
        }

        private fun mapAttachment(ffiAttachment: FfiAttachment): Attachment =
            Attachment(
                filename = ffiAttachment.filename ?: "",
                mimeType = ffiAttachment.mimeType,
                data = ffiAttachment.content.toByteString(),
            )

        private fun mapRemoteAttachment(ffiRemote: FfiRemoteAttachment): RemoteAttachment =
            RemoteAttachment(
                url = URL(ffiRemote.url),
                contentDigest = ffiRemote.contentDigest,
                secret = ffiRemote.secret.toByteString(),
                salt = ffiRemote.salt.toByteString(),
                nonce = ffiRemote.nonce.toByteString(),
                scheme = ffiRemote.scheme,
                contentLength = ffiRemote.contentLength.toInt(),
                filename = ffiRemote.filename,
            )

        private fun mapRemoteAttachmentInfo(ffiInfo: FfiRemoteAttachmentInfo): RemoteAttachmentInfo =
            RemoteAttachmentInfo(
                url = ffiInfo.url,
                filename = ffiInfo.filename ?: "",
                contentLength = ffiInfo.contentLength?.toLong() ?: 0,
                contentDigest = ffiInfo.contentDigest,
                nonce = ffiInfo.nonce.toByteString(),
                scheme = ffiInfo.scheme,
                salt = ffiInfo.salt.toByteString(),
                secret = ffiInfo.secret.toByteString(),
            )

        private fun mapMultiRemoteAttachment(ffiMulti: FfiMultiRemoteAttachment): MultiRemoteAttachment =
            MultiRemoteAttachment(
                remoteAttachments = ffiMulti.attachments.map { mapRemoteAttachmentInfo(it) },
            )

        private fun mapTransactionMetadata(meta: FfiTransactionMetadata): TransactionReference.Metadata =
            TransactionReference.Metadata(
                transactionType = meta.transactionType,
                currency = meta.currency,
                amount = meta.amount,
                decimals = meta.decimals,
                fromAddress = meta.fromAddress,
                toAddress = meta.toAddress,
            )

        private fun mapTransactionReference(ffiTx: FfiTransactionReference): TransactionReference =
            TransactionReference(
                namespace = ffiTx.namespace,
                networkId = ffiTx.networkId,
                reference = ffiTx.reference,
                metadata = ffiTx.metadata?.let { mapTransactionMetadata(it) },
            )

        // Helper functions for GroupUpdated proto mapping

        private fun mapFfiInboxToProto(
            ffiInbox: FfiInbox,
        ): org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated.Inbox =
            org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated.Inbox
                .newBuilder()
                .setInboxId(ffiInbox.inboxId)
                .build()

        private fun mapFfiMetadataChangeToProto(
            ffiChange: FfiMetadataFieldChange,
        ): org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated.MetadataFieldChange =
            org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated.MetadataFieldChange
                .newBuilder()
                .setFieldName(ffiChange.fieldName)
                .apply {
                    ffiChange.oldValue?.let { setOldValue(it) }
                    ffiChange.newValue?.let { setNewValue(it) }
                }.build()

        private fun mapGroupUpdated(
            ffiGroupUpdated: FfiGroupUpdated,
        ): org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated =
            org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated
                .newBuilder()
                .apply {
                    initiatedByInboxId = ffiGroupUpdated.initiatedByInboxId

                    // Add added inboxes
                    ffiGroupUpdated.addedInboxes.forEach { ffiInbox ->
                        addAddedInboxes(mapFfiInboxToProto(ffiInbox))
                    }

                    // Add removed inboxes
                    ffiGroupUpdated.removedInboxes.forEach { ffiInbox ->
                        addRemovedInboxes(mapFfiInboxToProto(ffiInbox))
                    }

                    // Add metadata field changes
                    ffiGroupUpdated.metadataFieldChanges.forEach { ffiChange ->
                        addMetadataFieldChanges(mapFfiMetadataChangeToProto(ffiChange))
                    }
                }.build()

        /**
         * Decode content from FfiDecodedMessageContent
         */
        internal fun decodeContent(content: FfiDecodedMessageContent): Any? =
            when (content) {
                is FfiDecodedMessageContent.Text -> content.v1.content
                is FfiDecodedMessageContent.Reaction -> mapReaction(content.v1)
                is FfiDecodedMessageContent.Reply -> Reply.create(content.v1)
                is FfiDecodedMessageContent.Attachment -> mapAttachment(content.v1)
                is FfiDecodedMessageContent.RemoteAttachment -> mapRemoteAttachment(content.v1)
                is FfiDecodedMessageContent.MultiRemoteAttachment ->
                    mapMultiRemoteAttachment(
                        content.v1,
                    )

                is FfiDecodedMessageContent.TransactionReference -> mapTransactionReference(content.v1)
                is FfiDecodedMessageContent.WalletSendCalls -> content.v1
                is FfiDecodedMessageContent.GroupUpdated -> mapGroupUpdated(content.v1)
                is FfiDecodedMessageContent.ReadReceipt -> ReadReceipt
                is FfiDecodedMessageContent.Custom -> {
                    val encodedContent = encodedContentFromFfi(content.v1)
                    encodedContent.decoded<Any>()
                }

                else -> null
            }

        /**
         * Decode content from FfiDecodedMessageBody (used by Reply)
         */
        internal fun decodeBodyContent(body: FfiDecodedMessageBody): Any? =
            when (body) {
                is FfiDecodedMessageBody.Text -> body.v1.content
                is FfiDecodedMessageBody.Reaction -> mapReaction(body.v1)
                is FfiDecodedMessageBody.Attachment -> mapAttachment(body.v1)
                is FfiDecodedMessageBody.RemoteAttachment -> mapRemoteAttachment(body.v1)
                is FfiDecodedMessageBody.MultiRemoteAttachment -> mapMultiRemoteAttachment(body.v1)
                is FfiDecodedMessageBody.TransactionReference -> mapTransactionReference(body.v1)
                is FfiDecodedMessageBody.WalletSendCalls -> body.v1
                is FfiDecodedMessageBody.GroupUpdated -> mapGroupUpdated(body.v1)
                is FfiDecodedMessageBody.Custom -> {
                    val encodedContent = encodedContentFromFfi(body.v1)
                    encodedContent.decoded<Any>()
                }

                else -> null
            }
    }
}
