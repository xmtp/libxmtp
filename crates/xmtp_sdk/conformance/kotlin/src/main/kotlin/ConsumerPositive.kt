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
