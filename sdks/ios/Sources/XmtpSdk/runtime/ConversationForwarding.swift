/// Generated from matching Group and Dm methods. Do not edit this output.
public extension Conversation {
    func addedByInboxId()  -> InboxId? {
        switch self {
        case .group(let group): return group.addedByInboxId()
        case .dm(let dm): return dm.addedByInboxId()
        }
    }

    func countMessages(options: ListMessagesOptions?) async throws  -> UInt64 {
        switch self {
        case .group(let group): return try await group.countMessages(options: options)
        case .dm(let dm): return try await dm.countMessages(options: options)
        }
    }

    func createdAt()  -> Timestamp {
        switch self {
        case .group(let group): return group.createdAt()
        case .dm(let dm): return dm.createdAt()
        }
    }

    func creatorInboxId()  -> InboxId? {
        switch self {
        case .group(let group): return group.creatorInboxId()
        case .dm(let dm): return dm.creatorInboxId()
        }
    }

    func debugInfo() async throws  -> ConversationDebugInfo {
        switch self {
        case .group(let group): return try await group.debugInfo()
        case .dm(let dm): return try await dm.debugInfo()
        }
    }

    func deleteMessage(id: MessageId) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.deleteMessage(id: id)
        case .dm(let dm): return try await dm.deleteMessage(id: id)
        }
    }

    func hmacKeys() async throws  -> [HmacKey] {
        switch self {
        case .group(let group): return try await group.hmacKeys()
        case .dm(let dm): return try await dm.hmacKeys()
        }
    }

    func id()  -> ConversationId {
        switch self {
        case .group(let group): return group.id()
        case .dm(let dm): return dm.id()
        }
    }

    func isCreator()  -> Bool {
        switch self {
        case .group(let group): return group.isCreator()
        case .dm(let dm): return dm.isCreator()
        }
    }

    func kind()  -> ConversationKind {
        switch self {
        case .group(let group): return group.kind()
        case .dm(let dm): return dm.kind()
        }
    }

    func lastActivityAt(contentTypes: [ContentTypeId]?) async throws  -> Timestamp {
        switch self {
        case .group(let group): return try await group.lastActivityAt(contentTypes: contentTypes)
        case .dm(let dm): return try await dm.lastActivityAt(contentTypes: contentTypes)
        }
    }

    func lastMessage() async throws  -> Message? {
        switch self {
        case .group(let group): return try await group.lastMessage()
        case .dm(let dm): return try await dm.lastMessage()
        }
    }

    func lastReadTimes() async throws  -> [String: Timestamp] {
        switch self {
        case .group(let group): return try await group.lastReadTimes()
        case .dm(let dm): return try await dm.lastReadTimes()
        }
    }

    func mapValue(field: MetadataFieldRef, key: FieldKey) async throws  -> FieldValue? {
        switch self {
        case .group(let group): return try await group.mapValue(field: field, key: key)
        case .dm(let dm): return try await dm.mapValue(field: field, key: key)
        }
    }

    func members() async throws  -> [Member] {
        switch self {
        case .group(let group): return try await group.members()
        case .dm(let dm): return try await dm.members()
        }
    }

    func messageHistoryPage(options: ListMessagesOptions?, before: MessageHistoryPosition?, after: MessageHistoryPosition?) async throws  -> MessageHistoryPage {
        switch self {
        case .group(let group): return try await group.messageHistoryPage(options: options, before: before, after: after)
        case .dm(let dm): return try await dm.messageHistoryPage(options: options, before: before, after: after)
        }
    }

    func messageHistorySnapshot(limit: UInt32) async throws  -> MessageHistorySnapshot {
        switch self {
        case .group(let group): return try await group.messageHistorySnapshot(limit: limit)
        case .dm(let dm): return try await dm.messageHistorySnapshot(limit: limit)
        }
    }

    func messageReader(options: ConversationMessageReaderOptions?) async throws  -> MessageReader {
        switch self {
        case .group(let group): return try await group.messageReader(options: options)
        case .dm(let dm): return try await dm.messageReader(options: options)
        }
    }

    func messages(options: ListMessagesOptions?) async throws  -> [Message] {
        switch self {
        case .group(let group): return try await group.messages(options: options)
        case .dm(let dm): return try await dm.messages(options: options)
        }
    }

    func metadataField(name: String) async throws  -> MetadataFieldDescriptor? {
        switch self {
        case .group(let group): return try await group.metadataField(name: name)
        case .dm(let dm): return try await dm.metadataField(name: name)
        }
    }

    func metadataFields() async throws  -> [MetadataFieldDescriptor] {
        switch self {
        case .group(let group): return try await group.metadataFields()
        case .dm(let dm): return try await dm.metadataFields()
        }
    }

    func metadataValue(field: MetadataFieldRef) async throws  -> MetadataValue? {
        switch self {
        case .group(let group): return try await group.metadataValue(field: field)
        case .dm(let dm): return try await dm.metadataValue(field: field)
        }
    }

    func metadataValues(fields: [MetadataFieldRef]) async throws  -> [MetadataFieldValue] {
        switch self {
        case .group(let group): return try await group.metadataValues(fields: fields)
        case .dm(let dm): return try await dm.metadataValues(fields: fields)
        }
    }

    func prepareMessage(encoded: EncodedContent, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.prepareMessage(encoded: encoded, options: options)
        case .dm(let dm): return try await dm.prepareMessage(encoded: encoded, options: options)
        }
    }

    func publishMessage(id: MessageId) async throws {
        switch self {
        case .group(let group): return try await group.publishMessage(id: id)
        case .dm(let dm): return try await dm.publishMessage(id: id)
        }
    }

    func publishMessages() async throws {
        switch self {
        case .group(let group): return try await group.publishMessages()
        case .dm(let dm): return try await dm.publishMessages()
        }
    }

    func send(encoded: EncodedContent, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.send(encoded: encoded, options: options)
        case .dm(let dm): return try await dm.send(encoded: encoded, options: options)
        }
    }

    func sendActions(actions: Actions, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendActions(actions: actions, options: options)
        case .dm(let dm): return try await dm.sendActions(actions: actions, options: options)
        }
    }

    func sendAttachment(attachment: Attachment, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendAttachment(attachment: attachment, options: options)
        case .dm(let dm): return try await dm.sendAttachment(attachment: attachment, options: options)
        }
    }

    func sendIntent(intent: Intent, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendIntent(intent: intent, options: options)
        case .dm(let dm): return try await dm.sendIntent(intent: intent, options: options)
        }
    }

    func sendMarkdown(markdown: String, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendMarkdown(markdown: markdown, options: options)
        case .dm(let dm): return try await dm.sendMarkdown(markdown: markdown, options: options)
        }
    }

    func sendMultiRemoteAttachment(attachment: MultiRemoteAttachment, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendMultiRemoteAttachment(attachment: attachment, options: options)
        case .dm(let dm): return try await dm.sendMultiRemoteAttachment(attachment: attachment, options: options)
        }
    }

    func sendReaction(reference: MessageId, referenceInboxId: InboxId?, reaction: Reaction, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendReaction(reference: reference, referenceInboxId: referenceInboxId, reaction: reaction, options: options)
        case .dm(let dm): return try await dm.sendReaction(reference: reference, referenceInboxId: referenceInboxId, reaction: reaction, options: options)
        }
    }

    func sendReadReceipt(options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendReadReceipt(options: options)
        case .dm(let dm): return try await dm.sendReadReceipt(options: options)
        }
    }

    func sendRemoteAttachment(attachment: RemoteAttachment, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendRemoteAttachment(attachment: attachment, options: options)
        case .dm(let dm): return try await dm.sendRemoteAttachment(attachment: attachment, options: options)
        }
    }

    func sendReply(reference: MessageId, referenceInboxId: InboxId?, content: EncodedContent, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendReply(reference: reference, referenceInboxId: referenceInboxId, content: content, options: options)
        case .dm(let dm): return try await dm.sendReply(reference: reference, referenceInboxId: referenceInboxId, content: content, options: options)
        }
    }

    func sendText(text: String, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendText(text: text, options: options)
        case .dm(let dm): return try await dm.sendText(text: text, options: options)
        }
    }

    func sendTransactionReference(reference: TransactionReference, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendTransactionReference(reference: reference, options: options)
        case .dm(let dm): return try await dm.sendTransactionReference(reference: reference, options: options)
        }
    }

    func sendWalletSendCalls(calls: WalletSendCalls, options: SendOptions?) async throws  -> MessageId {
        switch self {
        case .group(let group): return try await group.sendWalletSendCalls(calls: calls, options: options)
        case .dm(let dm): return try await dm.sendWalletSendCalls(calls: calls, options: options)
        }
    }

    func setNotifications(value: NotificationOverride) async throws {
        switch self {
        case .group(let group): return try await group.setNotifications(value: value)
        case .dm(let dm): return try await dm.setNotifications(value: value)
        }
    }

    func sync() async throws {
        switch self {
        case .group(let group): return try await group.sync()
        case .dm(let dm): return try await dm.sync()
        }
    }

    func topic()  -> String {
        switch self {
        case .group(let group): return group.topic()
        case .dm(let dm): return dm.topic()
        }
    }

    func updateConsentState(state: ConsentState) async throws {
        switch self {
        case .group(let group): return try await group.updateConsentState(state: state)
        case .dm(let dm): return try await dm.updateConsentState(state: state)
        }
    }

    func updateDisappearingSettings(settings: DisappearingSettings?) async throws {
        switch self {
        case .group(let group): return try await group.updateDisappearingSettings(settings: settings)
        case .dm(let dm): return try await dm.updateDisappearingSettings(settings: settings)
        }
    }

    func updateMetadataField(field: MetadataFieldRef, operation: ComponentMutation) async throws {
        switch self {
        case .group(let group): return try await group.updateMetadataField(field: field, operation: operation)
        case .dm(let dm): return try await dm.updateMetadataField(field: field, operation: operation)
        }
    }

    func updateUserData(values: [UserFieldUpdate]) async throws {
        switch self {
        case .group(let group): return try await group.updateUserData(values: values)
        case .dm(let dm): return try await dm.updateUserData(values: values)
        }
    }

    func userData(fields: [MetadataFieldRef]?, inboxIds: [InboxId]?) async throws  -> [InboxId: [UserFieldValue]] {
        switch self {
        case .group(let group): return try await group.userData(fields: fields, inboxIds: inboxIds)
        case .dm(let dm): return try await dm.userData(fields: fields, inboxIds: inboxIds)
        }
    }


}
