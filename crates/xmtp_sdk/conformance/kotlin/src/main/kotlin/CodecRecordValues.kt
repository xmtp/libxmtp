import uniffi.xmtp_sdk.*

// verifies: CTYPE-007, CTYPE-026
fun checkCodecRecordValues() {
    checkEncryptedRemoteAttachmentProjection()
    val reference: MessageId = "d".repeat(64)
    val inboxes = listOf<InboxId?>(null, "b".repeat(64))
    val reaction = Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE)
    val nested =
        TextCodec()
            .encode(
                "nested bytes",
            ).copy(parameters = mapOf("key" to "value"), fallback = "nested fallback")
    check(ReactionV2Content(reference, reaction = reaction).referenceInboxId == null)
    check(ReplyContent(reference, content = nested).referenceInboxId == null)
    for (inbox in inboxes) {
        val reactionValue = ReactionV2Content(reference, inbox, reaction)
        val reactionWire = encodeStandard(StandardContent.Reaction(reference, inbox, reaction))
        check(matchesRust(ReactionV2Codec(), reactionValue, reactionWire)) {
            "ReactionV2Content lost a field or changed wire bytes"
        }
        val replyValue = ReplyContent(reference, inbox, nested)
        val replyWire = encodeStandard(StandardContent.Reply(reference, inbox, nested))
        check(matchesRust(ReplyCodec(), replyValue, replyWire)) {
            "ReplyContent lost a field or changed wire bytes"
        }
        val decoded = ReplyCodec().decode(replyWire)
        check(decoded.content.type == nested.type)
        check(decoded.content.parameters == nested.parameters)
        check(decoded.content.fallback == nested.fallback)
        check(decoded.content.content.contentEquals(nested.content))
    }
    val deleteValue = DeleteMessageContent(reference)
    val deleteWire = encodeStandard(StandardContent.DeleteMessage(reference))
    check(matchesRust(DeleteMessageCodec(), deleteValue, deleteWire)) {
        "DeleteMessageContent lost its message id or changed wire bytes"
    }
    println("Kotlin codec records preserve every field, optional inbox, and Rust wire bytes")
}
