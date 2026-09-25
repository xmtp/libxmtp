package uniffi.xmtp_sdk

private fun codecValueError() =
    XmtpException.InvalidArgument(
        ErrorDetails("InvalidArgument", ErrorCategory.INPUT, false, "wrong standard codec value"),
    )

private fun <T : Any> decodePure(
    encoded: EncodedContent,
    take: (StandardContent) -> T?,
): T = take(decodeStandard(encoded)) ?: throw codecValueError()

class TextCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.TEXT)

    override fun encode(value: Any) = encodeStandard(StandardContent.Text(value as String))

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { (it as? StandardContent.Text)?.v1 }
}

class MarkdownCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.MARKDOWN)

    override fun encode(value: Any) = encodeStandard(StandardContent.Markdown(value as String))

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { (it as? StandardContent.Markdown)?.v1 }
}

class ReadReceiptCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.READ_RECEIPT)

    override fun encode(value: Any): EncodedContent {
        require(value == Unit)
        return encodeStandard(StandardContent.ReadReceipt)
    }

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) {
            if (it is StandardContent.ReadReceipt) Unit else null
        }
}

class ReactionV2Codec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.REACTION)

    override fun encode(value: Any) = encodeStandard(value as? StandardContent.Reaction ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { it as? StandardContent.Reaction }
}

class AttachmentCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.ATTACHMENT)

    override fun encode(value: Any) = encodeStandard(StandardContent.Attachment(value as Attachment))

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { (it as? StandardContent.Attachment)?.v1 }
}

class RemoteAttachmentCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.REMOTE_ATTACHMENT)

    override fun encode(value: Any) = encodeStandard(StandardContent.RemoteAttachment(value as RemoteAttachment))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) {
            (it as? StandardContent.RemoteAttachment)?.v1
        }
}

class MultiRemoteAttachmentCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.MULTI_REMOTE_ATTACHMENT)

    override fun encode(value: Any) =
        encodeStandard(StandardContent.MultiRemoteAttachment(value as MultiRemoteAttachment))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) {
            (it as? StandardContent.MultiRemoteAttachment)?.v1
        }
}

class TransactionReferenceCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.TRANSACTION_REFERENCE)

    override fun encode(value: Any) =
        encodeStandard(StandardContent.TransactionReference(value as TransactionReference))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) {
            (it as? StandardContent.TransactionReference)?.v1
        }
}

class WalletSendCallsCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.WALLET_SEND_CALLS)

    override fun encode(value: Any) = encodeStandard(StandardContent.WalletSendCalls(value as WalletSendCalls))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) { (it as? StandardContent.WalletSendCalls)?.v1 }
}

class ActionsCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.ACTIONS)

    override fun encode(value: Any) = encodeStandard(StandardContent.Actions(value as Actions))

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { (it as? StandardContent.Actions)?.v1 }
}

class IntentCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.INTENT)

    override fun encode(value: Any) = encodeStandard(StandardContent.Intent(value as Intent))

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { (it as? StandardContent.Intent)?.v1 }
}

class ReplyCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.REPLY)

    override fun encode(value: Any) = encodeStandard(value as? StandardContent.Reply ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { it as? StandardContent.Reply }
}

class GroupUpdatedCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.GROUP_UPDATED)

    override fun encode(value: Any) = encodeStandard(StandardContent.GroupUpdated(value as GroupUpdated))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) { (it as? StandardContent.GroupUpdated)?.v1 }
}

class DeleteMessageCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.DELETE_MESSAGE)

    override fun encode(value: Any) = encodeStandard(value as? StandardContent.DeleteMessage ?: throw codecValueError())

    override fun decode(encoded: EncodedContent): Any = decodePure(encoded) { it as? StandardContent.DeleteMessage }
}

class LeaveRequestCodec : SDKContentCodec {
    override val type get() = standardContentType(StandardContentKind.LEAVE_REQUEST)

    override fun encode(value: Any) = encodeStandard(StandardContent.LeaveRequest(value as LeaveRequest))

    override fun decode(encoded: EncodedContent): Any =
        decodePure(encoded) { (it as? StandardContent.LeaveRequest)?.v1 }
}
