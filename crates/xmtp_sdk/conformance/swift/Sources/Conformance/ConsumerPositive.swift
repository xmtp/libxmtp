import XmtpSdk

func consumeInfallibleListenerStop(_ host: SDKClient, _ raw: Client, _ id: ListenerId) async {
    await host.stopListener(id)
    await raw.stopListener(id: id)
}

func consumeOmittedSendOptions(
    _ group: Group, _ conversations: Conversations, _ id: MessageId,
    _ reaction: Reaction, _ encoded: EncodedContent
) async throws {
    _ = try await group.send(encoded: encoded)
    _ = try await group.prepareMessage(encoded: encoded)
    _ = try await conversations.reactToMessage(id: id, reaction: reaction)
    _ = try await conversations.replyToMessage(id: id, content: encoded)
}

func consumeOmittedTypedSendOptions(
    _ group: Group, _ id: MessageId, _ reaction: Reaction, _ encoded: EncodedContent,
    _ attachment: Attachment, _ remote: RemoteAttachment, _ multiRemote: MultiRemoteAttachment,
    _ transaction: TransactionReference, _ walletCalls: WalletSendCalls,
    _ actions: Actions, _ intent: Intent
) async throws {
    _ = try await group.sendText(text: "text")
    _ = try await group.sendMarkdown(markdown: "markdown")
    _ = try await group.sendReaction(reference: id, referenceInboxId: nil, reaction: reaction)
    _ = try await group.sendReply(reference: id, referenceInboxId: nil, content: encoded)
    _ = try await group.sendReadReceipt()
    _ = try await group.sendAttachment(attachment: attachment)
    _ = try await group.sendRemoteAttachment(attachment: remote)
    _ = try await group.sendMultiRemoteAttachment(attachment: multiRemote)
    _ = try await group.sendTransactionReference(reference: transaction)
    _ = try await group.sendWalletSendCalls(calls: walletCalls)
    _ = try await group.sendActions(actions: actions)
    _ = try await group.sendIntent(intent: intent)
}

func consumePositive(_ id: ConversationId, _ conversation: Conversation, _ content: MessageContent) -> ConversationId {
    let narrowed: ConversationId
    switch conversation {
    case let .group(group): narrowed = group.id()
    case let .dm(dm): narrowed = dm.id()
    }
    if case let .custom(encoded, _) = content {
        let _: EncodedContent = encoded
    }
    return narrowed == id ? id : narrowed
}

func consumeStandardIds(_ content: StandardContent) -> MessageId? {
    switch content {
    case let .reaction(reference, inboxId, _):
        let _: InboxId? = inboxId
        return reference
    case let .reply(reference, inboxId, _):
        let _: InboxId? = inboxId
        return reference
    case let .deleteMessage(messageId):
        let id: MessageId = messageId
        return id
    default:
        return nil
    }
}
