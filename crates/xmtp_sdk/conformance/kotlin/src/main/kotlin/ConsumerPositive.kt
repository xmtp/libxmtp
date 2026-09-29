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
