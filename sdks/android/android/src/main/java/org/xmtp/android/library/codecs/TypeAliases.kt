package org.xmtp.android.library.codecs

// Re-export all shared codec types under the old package name for backwards compatibility

// AttachmentCodec.kt
typealias Attachment = org.xmtp.kotlin.codecs.Attachment
typealias AttachmentCodec = org.xmtp.kotlin.codecs.AttachmentCodec

// ContentCodec.kt
typealias EncodedContent = org.xmtp.kotlin.codecs.EncodedContent
typealias ContentCodec<T> = org.xmtp.kotlin.codecs.ContentCodec<T>

// ContentTypeId.kt
typealias ContentTypeId = org.xmtp.kotlin.codecs.ContentTypeId
typealias ContentTypeIdBuilder = org.xmtp.kotlin.codecs.ContentTypeIdBuilder

// DeletedMessage.kt
typealias DeletedMessage = org.xmtp.kotlin.codecs.DeletedMessage
typealias DeletedBy = org.xmtp.kotlin.codecs.DeletedBy

// DeleteMessageCodec.kt
typealias DeleteMessageRequest = org.xmtp.kotlin.codecs.DeleteMessageRequest
typealias DeleteMessageCodec = org.xmtp.kotlin.codecs.DeleteMessageCodec

// GroupUpdatedCodec.kt
typealias GroupUpdated = org.xmtp.kotlin.codecs.GroupUpdated
typealias GroupUpdatedCodec = org.xmtp.kotlin.codecs.GroupUpdatedCodec

// LeaveRequestCodec.kt
typealias LeaveRequest = org.xmtp.kotlin.codecs.LeaveRequest
typealias LeaveRequestCodec = org.xmtp.kotlin.codecs.LeaveRequestCodec

// MultiRemoteAttachmentCodec.kt
typealias MultiRemoteAttachment = org.xmtp.kotlin.codecs.MultiRemoteAttachment
typealias RemoteAttachmentInfo = org.xmtp.kotlin.codecs.RemoteAttachmentInfo
typealias MultiRemoteAttachmentCodec = org.xmtp.kotlin.codecs.MultiRemoteAttachmentCodec

// ReactionCodec.kt
typealias Reaction = org.xmtp.kotlin.codecs.Reaction
typealias ReactionAction = org.xmtp.kotlin.codecs.ReactionAction
typealias ReactionSchema = org.xmtp.kotlin.codecs.ReactionSchema
typealias ReactionCodec = org.xmtp.kotlin.codecs.ReactionCodec

// ReactionV2Codec.kt
typealias ReactionV2Codec = org.xmtp.kotlin.codecs.ReactionV2Codec

// ReadReceiptCodec.kt
typealias ReadReceipt = org.xmtp.kotlin.codecs.ReadReceipt
typealias ReadReceiptCodec = org.xmtp.kotlin.codecs.ReadReceiptCodec

// RemoteAttachmentCodec.kt
typealias EncryptedEncodedContent = org.xmtp.kotlin.codecs.EncryptedEncodedContent
typealias RemoteAttachment = org.xmtp.kotlin.codecs.RemoteAttachment
typealias Fetcher = org.xmtp.kotlin.codecs.Fetcher
typealias HTTPFetcher = org.xmtp.kotlin.codecs.HTTPFetcher
typealias RemoteAttachmentCodec = org.xmtp.kotlin.codecs.RemoteAttachmentCodec

// ReplyCodec.kt
typealias Reply = org.xmtp.kotlin.codecs.Reply
typealias ReplyCodec = org.xmtp.kotlin.codecs.ReplyCodec

// TextCodec.kt
typealias TextCodec = org.xmtp.kotlin.codecs.TextCodec

// TransactionReferenceCodec.kt
typealias TransactionReference = org.xmtp.kotlin.codecs.TransactionReference
typealias TransactionReferenceCodec = org.xmtp.kotlin.codecs.TransactionReferenceCodec

// WalletSendCalls.kt
typealias WalletSendCalls = org.xmtp.kotlin.codecs.WalletSendCalls
