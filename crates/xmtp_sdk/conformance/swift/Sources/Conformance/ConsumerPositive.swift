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

func consumeOmittedTypedSendOptions(
    _ group: Group, _ id: MessageID, _ reaction: Reaction, _ encoded: EncodedContent,
    _ attachment: Attachment, _ remote: RemoteAttachment, _ multiRemote: MultiRemoteAttachment,
    _ transaction: TransactionReference, _ walletCalls: WalletSendCalls,
    _ actions: Actions, _ intent: Intent
) async throws {
    _ = try await group.sendText(text: "text")
    _ = try await group.sendMarkdown(markdown: "markdown")
    _ = try await group.sendReaction(reference: id, referenceInboxID: nil, reaction: reaction)
    _ = try await group.sendReply(reference: id, referenceInboxID: nil, content: encoded)
    _ = try await group.sendReadReceipt()
    _ = try await group.sendAttachment(attachment: attachment)
    _ = try await group.sendRemoteAttachment(attachment: remote)
    _ = try await group.sendMultiRemoteAttachment(attachment: multiRemote)
    _ = try await group.sendTransactionReference(reference: transaction)
    _ = try await group.sendWalletSendCalls(calls: walletCalls)
    _ = try await group.sendActions(actions: actions)
    _ = try await group.sendIntent(intent: intent)
}

func consumePositive(_ id: ConversationID, _ conversation: Conversation, _ content: MessageContent) -> ConversationID {
    let narrowed: ConversationID
    switch conversation {
    case let .group(group): narrowed = group.id()
    case let .dm(dm): narrowed = dm.id()
    }
    if case let .custom(encoded, _) = content {
        let _: EncodedContent = encoded
    }
    return narrowed == id ? id : narrowed
}

func consumeStandardIDs(_ content: StandardContent) -> MessageID? {
    switch content {
    case let .reaction(reference, inboxID, _):
        let _: InboxID? = inboxID
        return reference
    case let .reply(reference, inboxID, _):
        let _: InboxID? = inboxID
        return reference
    case let .deleteMessage(messageID):
        let id: MessageID = messageID
        return id
    default:
        return nil
    }
}
