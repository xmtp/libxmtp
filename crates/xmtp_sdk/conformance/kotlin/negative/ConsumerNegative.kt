import uniffi.xmtp_sdk.*

fun consumeNegative(
    conversation: Conversation,
    content: MessageContent,
) {
    val id: ConversationID = "raw string"
    val encoded: EncodedContent = content
    val group: Group = conversation
    println("$id $encoded $group")
}
