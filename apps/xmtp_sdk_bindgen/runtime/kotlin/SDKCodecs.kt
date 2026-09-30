package uniffi.xmtp_sdk

// The standard codecs. Each has the value type of its standard content, and
// Rust encodes and decodes the bytes. The reaction, reply, and delete-message
// codecs take the whole StandardContent and reject another variant at run time
// (a known P9 gap; see the Ref).

private fun codecValueError() =
    XmtpException.InvalidArgument(
        ErrorDetails("InvalidArgument", ErrorCategory.INPUT, false, "wrong standard codec value"),
    )

private fun <T : Any> standardValue(
    encoded: EncodedContent,
    take: (StandardContent) -> T?,
): T = take(decodeStandard(encoded)) ?: throw codecValueError()

class TextCodec : ContentCodec<String> {
    override val type get() = standardContentType(StandardContentKind.TEXT)

    override fun encode(value: String) = encodeStandard(StandardContent.Text(value))

    override fun decode(encoded: EncodedContent): String = standardValue(encoded) { (it as? StandardContent.Text)?.v1 }
}

class MarkdownCodec : ContentCodec<String> {
    override val type get() = standardContentType(StandardContentKind.MARKDOWN)

    override fun encode(value: String) = encodeStandard(StandardContent.Markdown(value))

    override fun decode(encoded: EncodedContent): String =
        standardValue(encoded) { (it as? StandardContent.Markdown)?.v1 }
}

class ReadReceiptCodec : ContentCodec<Unit> {
    override val type get() = standardContentType(StandardContentKind.READ_RECEIPT)

    override fun encode(value: Unit) = encodeStandard(StandardContent.ReadReceipt)

    override fun decode(encoded: EncodedContent): Unit =
        standardValue(encoded) {
            if (it is StandardContent.ReadReceipt) Unit else null
        }
}

class ReactionV2Codec : ContentCodec<StandardContent> {
    override val type get() = standardContentType(StandardContentKind.REACTION)

    override fun encode(value: StandardContent) =
        encodeStandard(value as? StandardContent.Reaction ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): StandardContent =
        standardValue(encoded) {
            it as? StandardContent.Reaction
        }
}

class AttachmentCodec : ContentCodec<Attachment> {
    override val type get() = standardContentType(StandardContentKind.ATTACHMENT)

    override fun encode(value: Attachment) = encodeStandard(StandardContent.Attachment(value))

    override fun decode(encoded: EncodedContent): Attachment =
        standardValue(encoded) {
            (it as? StandardContent.Attachment)?.v1
        }
}

class RemoteAttachmentCodec : ContentCodec<RemoteAttachment> {
    override val type get() = standardContentType(StandardContentKind.REMOTE_ATTACHMENT)

    override fun encode(value: RemoteAttachment) = encodeStandard(StandardContent.RemoteAttachment(value))

    override fun decode(encoded: EncodedContent): RemoteAttachment =
        standardValue(encoded) {
            (it as? StandardContent.RemoteAttachment)?.v1
        }
}

class MultiRemoteAttachmentCodec : ContentCodec<MultiRemoteAttachment> {
    override val type get() = standardContentType(StandardContentKind.MULTI_REMOTE_ATTACHMENT)

    override fun encode(value: MultiRemoteAttachment) = encodeStandard(StandardContent.MultiRemoteAttachment(value))

    override fun decode(encoded: EncodedContent): MultiRemoteAttachment =
        standardValue(encoded) {
            (it as? StandardContent.MultiRemoteAttachment)?.v1
        }
}

class TransactionReferenceCodec : ContentCodec<TransactionReference> {
    override val type get() = standardContentType(StandardContentKind.TRANSACTION_REFERENCE)

    override fun encode(value: TransactionReference) = encodeStandard(StandardContent.TransactionReference(value))

    override fun decode(encoded: EncodedContent): TransactionReference =
        standardValue(encoded) {
            (it as? StandardContent.TransactionReference)?.v1
        }
}

class WalletSendCallsCodec : ContentCodec<WalletSendCalls> {
    override val type get() = standardContentType(StandardContentKind.WALLET_SEND_CALLS)

    override fun encode(value: WalletSendCalls) = encodeStandard(StandardContent.WalletSendCalls(value))

    override fun decode(encoded: EncodedContent): WalletSendCalls =
        standardValue(encoded) {
            (it as? StandardContent.WalletSendCalls)?.v1
        }
}

class ActionsCodec : ContentCodec<Actions> {
    override val type get() = standardContentType(StandardContentKind.ACTIONS)

    override fun encode(value: Actions) = encodeStandard(StandardContent.Actions(value))

    override fun decode(encoded: EncodedContent): Actions =
        standardValue(encoded) { (it as? StandardContent.Actions)?.v1 }
}

class IntentCodec : ContentCodec<Intent> {
    override val type get() = standardContentType(StandardContentKind.INTENT)

    override fun encode(value: Intent) = encodeStandard(StandardContent.Intent(value))

    override fun decode(encoded: EncodedContent): Intent =
        standardValue(encoded) { (it as? StandardContent.Intent)?.v1 }
}

class ReplyCodec : ContentCodec<StandardContent> {
    override val type get() = standardContentType(StandardContentKind.REPLY)

    override fun encode(value: StandardContent) =
        encodeStandard(value as? StandardContent.Reply ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): StandardContent =
        standardValue(encoded) { it as? StandardContent.Reply }
}

class GroupUpdatedCodec : ContentCodec<GroupUpdated> {
    override val type get() = standardContentType(StandardContentKind.GROUP_UPDATED)

    override fun encode(value: GroupUpdated) = encodeStandard(StandardContent.GroupUpdated(value))

    override fun decode(encoded: EncodedContent): GroupUpdated =
        standardValue(encoded) {
            (it as? StandardContent.GroupUpdated)?.v1
        }
}

class DeleteMessageCodec : ContentCodec<StandardContent> {
    override val type get() = standardContentType(StandardContentKind.DELETE_MESSAGE)

    override fun encode(value: StandardContent) =
        encodeStandard(value as? StandardContent.DeleteMessage ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): StandardContent =
        standardValue(encoded) {
            it as? StandardContent.DeleteMessage
        }
}

class LeaveRequestCodec : ContentCodec<LeaveRequest> {
    override val type get() = standardContentType(StandardContentKind.LEAVE_REQUEST)

    override fun encode(value: LeaveRequest) = encodeStandard(StandardContent.LeaveRequest(value))

    override fun decode(encoded: EncodedContent): LeaveRequest =
        standardValue(encoded) {
            (it as? StandardContent.LeaveRequest)?.v1
        }
}
