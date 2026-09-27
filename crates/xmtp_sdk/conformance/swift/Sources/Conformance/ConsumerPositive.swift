import XmtpSdk

func consumeOmittedSendOptions(
    _ group: Group, _ conversations: Conversations, _ id: MessageID,
    _ reaction: Reaction, _ encoded: EncodedContent
) async throws {
    _ = try await group.send(encoded: encoded)
    _ = try await group.prepareMessage(encoded: encoded)
    _ = try await conversations.reactToMessage(id: id, reaction: reaction)
    _ = try await conversations.replyToMessage(id: id, content: encoded)
}

func consumePositive(_ id: ConversationID, _ conversation: Conversation, _ content: MessageContent) -> ConversationID {
    let narrowed: ConversationID
    switch conversation {
    case let .group(group): narrowed = group.id()
    case let .dm(dm): narrowed = dm.id()
    }
    if case let .custom(encoded) = content {
        let _: EncodedContent = encoded
    }
    return narrowed == id ? id : narrowed
}
