import uniffi.xmtp_sdk.*

fun consumePositive(
    id: ConversationID,
    conversation: Conversation,
    content: MessageContent,
): ConversationID {
    val narrowed: ConversationID =
        when (conversation) {
            is Conversation.Group -> conversation.group.id()
            is Conversation.Dm -> conversation.dm.id()
        }
    if (content is MessageContent.Custom) {
        val encoded: EncodedContent = content.encoded
        check(encoded.type.typeID.isNotEmpty())
    }
    return if (narrowed == id) id else narrowed
}

fun consumeStandardIDs(content: StandardContent): MessageID? =
    when (content) {
        is StandardContent.Reaction -> {
            val inbox: InboxID? = content.referenceInboxID
            check(inbox == null || inbox.toString().isNotEmpty())
            content.reference
        }

        is StandardContent.Reply -> {
            content.reference
        }

        is StandardContent.DeleteMessage -> {
            content.messageID
        }

        else -> {
            null
        }
    }
