import uniffi.xmtp_sdk.*

fun consumePositive(
    id: ConversationId,
    conversation: Conversation,
    content: MessageContent,
): ConversationId {
    val narrowed: ConversationId =
        when (conversation) {
            is Conversation.Group -> conversation.group.id()
            is Conversation.Dm -> conversation.dm.id()
        }
    if (content is MessageContent.Custom) {
        val encoded: EncodedContent = content.encoded
        check(encoded.type.typeId.isNotEmpty())
    }
    return if (narrowed == id) id else narrowed
}

fun consumeStandardIds(content: StandardContent): MessageId? =
    when (content) {
        is StandardContent.Reaction -> {
            val inbox: InboxId? = content.referenceInboxId
            check(inbox == null || inbox.toString().isNotEmpty())
            content.reference
        }

        is StandardContent.Reply -> {
            content.reference
        }

        is StandardContent.DeleteMessage -> {
            content.messageId
        }

        else -> {
            null
        }
    }

fun receivedDetails(message: Message): String? {
    val raw: ByteArray = message.rawBytes
    val encoded: EncodedContent? = message.encoded
    val type: ContentTypeId? = message.contentType
    check(raw.isNotEmpty() || encoded == null || type != null)
    return when (val content = message.content) {
        is SDKMessageContent.Unknown -> content.error.code
        is SDKMessageContent.Custom -> content.error?.code
        is SDKMessageContent.Standard -> null
    }
}
