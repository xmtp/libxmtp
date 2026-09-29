import uniffi.xmtp_sdk.*

fun consumeNegative(
    conversation: Conversation,
    content: MessageContent,
) {
    val id: ConversationId = 42
    val removedFactory = MessageId.fromString("bad")
    val encoded: EncodedContent = content
    val group: Group = conversation
    val invalid = StandardContent.DeleteMessage(42)
    println("$id $removedFactory $encoded $group $invalid")
}
