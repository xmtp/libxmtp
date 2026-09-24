import XmtpSdk

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
