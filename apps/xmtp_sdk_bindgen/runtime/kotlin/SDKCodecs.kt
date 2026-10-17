package uniffi.xmtp_sdk

// Standard codecs use their own value types. Rust encodes and decodes the bytes.
// Reaction, reply, and delete-message codecs use their generated records.

private fun codecValueError() =
    XmtpException.InvalidArgument(
        ErrorDetails("InvalidArgument", ErrorCategory.INPUT, false, "wrong standard codec value"),
    )

private fun <T : Any> standardValue(
    encoded: EncodedContent,
    take: (StandardContent) -> T?,
): T = take(decodeStandard(encoded)) ?: throw codecValueError()

class TextCodec : ContentCodec<String> {
    override fun fallback(value: String): String? = encode(value).fallback

    override fun shouldPush(value: String): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.TEXT)

    override fun encode(value: String) = encodeStandard(StandardContent.Text(value))

    override fun decode(encoded: EncodedContent): String = standardValue(encoded) { (it as? StandardContent.Text)?.v1 }
}

class MarkdownCodec : ContentCodec<String> {
    override fun fallback(value: String): String? = encode(value).fallback

    override fun shouldPush(value: String): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.MARKDOWN)

    override fun encode(value: String) = encodeStandard(StandardContent.Markdown(value))

    override fun decode(encoded: EncodedContent): String =
        standardValue(encoded) { (it as? StandardContent.Markdown)?.v1 }
}

class ReadReceiptCodec : ContentCodec<Unit> {
    override fun fallback(value: Unit): String? = encode(value).fallback

    override fun shouldPush(value: Unit): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.READ_RECEIPT)

    override fun encode(value: Unit) = encodeStandard(StandardContent.ReadReceipt)

    override fun decode(encoded: EncodedContent): Unit =
        standardValue(encoded) {
            if (it is StandardContent.ReadReceipt) Unit else null
        }
}

class ReactionV2Codec : ContentCodec<ReactionV2Content> {
    override fun fallback(value: ReactionV2Content): String? = encode(value).fallback

    override fun shouldPush(value: ReactionV2Content): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.REACTION)

    override fun encode(value: ReactionV2Content) =
        encodeStandard(StandardContent.Reaction(value.reference, value.referenceInboxId, value.reaction))

    override fun decode(encoded: EncodedContent): ReactionV2Content =
        standardValue(encoded) {
            (it as? StandardContent.Reaction)?.let { value ->
                ReactionV2Content(value.reference, value.referenceInboxId, value.reaction)
            }
        }
}

class AttachmentCodec : ContentCodec<Attachment> {
    override fun fallback(value: Attachment): String? = encode(value).fallback

    override fun shouldPush(value: Attachment): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.ATTACHMENT)

    override fun encode(value: Attachment) = encodeStandard(StandardContent.Attachment(value))

    override fun decode(encoded: EncodedContent): Attachment =
        standardValue(encoded) {
            (it as? StandardContent.Attachment)?.v1
        }
}

class RemoteAttachmentCodec : ContentCodec<RemoteAttachment> {
    override fun fallback(value: RemoteAttachment): String? = encode(value).fallback

    override fun shouldPush(value: RemoteAttachment): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.REMOTE_ATTACHMENT)

    override fun encode(value: RemoteAttachment) = encodeStandard(StandardContent.RemoteAttachment(value))

    override fun decode(encoded: EncodedContent): RemoteAttachment =
        standardValue(encoded) {
            (it as? StandardContent.RemoteAttachment)?.v1
        }
}

class MultiRemoteAttachmentCodec : ContentCodec<MultiRemoteAttachment> {
    override fun fallback(value: MultiRemoteAttachment): String? = encode(value).fallback

    override fun shouldPush(value: MultiRemoteAttachment): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.MULTI_REMOTE_ATTACHMENT)

    override fun encode(value: MultiRemoteAttachment) = encodeStandard(StandardContent.MultiRemoteAttachment(value))

    override fun decode(encoded: EncodedContent): MultiRemoteAttachment =
        standardValue(encoded) {
            (it as? StandardContent.MultiRemoteAttachment)?.v1
        }
}

class TransactionReferenceCodec : ContentCodec<TransactionReference> {
    override fun fallback(value: TransactionReference): String? = encode(value).fallback

    override fun shouldPush(value: TransactionReference): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.TRANSACTION_REFERENCE)

    override fun encode(value: TransactionReference) = encodeStandard(StandardContent.TransactionReference(value))

    override fun decode(encoded: EncodedContent): TransactionReference =
        standardValue(encoded) {
            (it as? StandardContent.TransactionReference)?.v1
        }
}

class WalletSendCallsCodec : ContentCodec<WalletSendCalls> {
    override fun fallback(value: WalletSendCalls): String? = encode(value).fallback

    override fun shouldPush(value: WalletSendCalls): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.WALLET_SEND_CALLS)

    override fun encode(value: WalletSendCalls) = encodeStandard(StandardContent.WalletSendCalls(value))

    override fun decode(encoded: EncodedContent): WalletSendCalls =
        standardValue(encoded) {
            (it as? StandardContent.WalletSendCalls)?.v1
        }
}

class ActionsCodec : ContentCodec<Actions> {
    override fun fallback(value: Actions): String? = encode(value).fallback

    override fun shouldPush(value: Actions): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.ACTIONS)

    override fun encode(value: Actions) = encodeStandard(StandardContent.Actions(value))

    override fun decode(encoded: EncodedContent): Actions =
        standardValue(encoded) { (it as? StandardContent.Actions)?.v1 }
}

class IntentCodec : ContentCodec<Intent> {
    override fun fallback(value: Intent): String? = encode(value).fallback

    override fun shouldPush(value: Intent): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.INTENT)

    override fun encode(value: Intent) = encodeStandard(StandardContent.Intent(value))

    override fun decode(encoded: EncodedContent): Intent =
        standardValue(encoded) { (it as? StandardContent.Intent)?.v1 }
}

class ReplyCodec : ContentCodec<ReplyContent> {
    override fun fallback(value: ReplyContent): String? = encode(value).fallback

    override fun shouldPush(value: ReplyContent): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.REPLY)

    override fun encode(value: ReplyContent) =
        encodeStandard(StandardContent.Reply(value.reference, value.referenceInboxId, value.content))

    override fun decode(encoded: EncodedContent): ReplyContent =
        standardValue(encoded) {
            (it as? StandardContent.Reply)?.let { value ->
                ReplyContent(value.reference, value.referenceInboxId, value.content)
            }
        }
}

class GroupUpdatedCodec : ContentCodec<GroupUpdated> {
    override fun fallback(value: GroupUpdated): String? = encode(value).fallback

    override fun shouldPush(value: GroupUpdated): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.GROUP_UPDATED)

    override fun encode(value: GroupUpdated) = encodeStandard(StandardContent.GroupUpdated(value))

    override fun decode(encoded: EncodedContent): GroupUpdated =
        standardValue(encoded) {
            (it as? StandardContent.GroupUpdated)?.v1
        }
}

class DeleteMessageCodec : ContentCodec<DeleteMessageContent> {
    override fun fallback(value: DeleteMessageContent): String? = encode(value).fallback

    override fun shouldPush(value: DeleteMessageContent): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.DELETE_MESSAGE)

    override fun encode(value: DeleteMessageContent) = encodeStandard(StandardContent.DeleteMessage(value.messageId))

    override fun decode(encoded: EncodedContent): DeleteMessageContent =
        standardValue(encoded) {
            (it as? StandardContent.DeleteMessage)?.let { value ->
                DeleteMessageContent(value.messageId)
            }
        }
}

class LeaveRequestCodec : ContentCodec<LeaveRequest> {
    override fun fallback(value: LeaveRequest): String? = encode(value).fallback

    override fun shouldPush(value: LeaveRequest): Boolean = catalogueContentTypeShouldPush(type)

    override val type get() = standardContentType(StandardContentKind.LEAVE_REQUEST)

    override fun encode(value: LeaveRequest) = encodeStandard(StandardContent.LeaveRequest(value))

    override fun decode(encoded: EncodedContent): LeaveRequest =
        standardValue(encoded) {
            (it as? StandardContent.LeaveRequest)?.v1
        }
}
