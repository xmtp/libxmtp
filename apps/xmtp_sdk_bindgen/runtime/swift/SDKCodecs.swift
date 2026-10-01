import Foundation

// The standard codecs. Each has the value type of its standard content, and
// Rust encodes and decodes the bytes. The reaction, reply, and delete-message
// codecs take the whole StandardContent and reject another variant at run time
// (a known P9 gap; see the Ref).

private func codecValueError() -> XmtpError {
    .InvalidArgument(ErrorDetails(code: "InvalidArgument", category: .input, retryable: false, message: "wrong standard codec value"))
}

private func standardValue<T>(_ encoded: EncodedContent, _ take: (StandardContent) -> T?) throws -> T {
    guard let value = try take(decodeStandard(encoded: encoded)) else { throw codecValueError() }
    return value
}

public struct TextCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .text)
    }

    public func encode(_ value: String) throws -> EncodedContent {
        try encodeStandard(value: .text(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> String {
        try standardValue(encoded) {
            if case let .text(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct MarkdownCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .markdown)
    }

    public func encode(_ value: String) throws -> EncodedContent {
        try encodeStandard(value: .markdown(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> String {
        try standardValue(encoded) {
            if case let .markdown(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct ReadReceiptCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .readReceipt)
    }

    public func encode(_: Void) throws -> EncodedContent {
        try encodeStandard(value: .readReceipt)
    }

    public func decode(_ encoded: EncodedContent) throws {
        try standardValue(encoded) {
            if case .readReceipt = $0 {
                ()
            } else {
                nil
            }
        }
    }
}

public struct ReactionV2Content: Sendable {
    public let reference: MessageId
    public let referenceInboxId: InboxId?
    public let reaction: Reaction

    public init(reference: MessageId, referenceInboxId: InboxId? = nil, reaction: Reaction) {
        self.reference = reference
        self.referenceInboxId = referenceInboxId
        self.reaction = reaction
    }
}

public struct ReplyContent: Sendable {
    public let reference: MessageId
    public let referenceInboxId: InboxId?
    public let content: EncodedContent

    public init(reference: MessageId, referenceInboxId: InboxId? = nil, content: EncodedContent) {
        self.reference = reference
        self.referenceInboxId = referenceInboxId
        self.content = content
    }
}

public struct DeleteMessageContent: Sendable {
    public let messageId: MessageId

    public init(messageId: MessageId) {
        self.messageId = messageId
    }
}

public struct ReactionV2Codec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .reaction)
    }

    public func encode(_ value: ReactionV2Content) throws -> EncodedContent {
        try encodeStandard(value: .reaction(reference: value.reference, referenceInboxId: value.referenceInboxId, reaction: value.reaction))
    }

    public func decode(_ encoded: EncodedContent) throws -> ReactionV2Content {
        try standardValue(encoded) {
            if case let .reaction(reference, referenceInboxId, reaction) = $0 {
                ReactionV2Content(reference: reference, referenceInboxId: referenceInboxId, reaction: reaction)
            } else {
                nil
            }
        }
    }
}

public struct AttachmentCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .attachment)
    }

    public func encode(_ value: Attachment) throws -> EncodedContent {
        try encodeStandard(value: .attachment(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> Attachment {
        try standardValue(encoded) {
            if case let .attachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct RemoteAttachmentCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .remoteAttachment)
    }

    public func encode(_ value: RemoteAttachment) throws -> EncodedContent {
        try encodeStandard(value: .remoteAttachment(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> RemoteAttachment {
        try standardValue(encoded) {
            if case let .remoteAttachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct MultiRemoteAttachmentCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .multiRemoteAttachment)
    }

    public func encode(_ value: MultiRemoteAttachment) throws -> EncodedContent {
        try encodeStandard(value: .multiRemoteAttachment(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> MultiRemoteAttachment {
        try standardValue(encoded) {
            if case let .multiRemoteAttachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct TransactionReferenceCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .transactionReference)
    }

    public func encode(_ value: TransactionReference) throws -> EncodedContent {
        try encodeStandard(value: .transactionReference(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> TransactionReference {
        try standardValue(encoded) {
            if case let .transactionReference(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct WalletSendCallsCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .walletSendCalls)
    }

    public func encode(_ value: WalletSendCalls) throws -> EncodedContent {
        try encodeStandard(value: .walletSendCalls(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> WalletSendCalls {
        try standardValue(encoded) {
            if case let .walletSendCalls(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct ActionsCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .actions)
    }

    public func encode(_ value: Actions) throws -> EncodedContent {
        try encodeStandard(value: .actions(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> Actions {
        try standardValue(encoded) {
            if case let .actions(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct IntentCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .intent)
    }

    public func encode(_ value: Intent) throws -> EncodedContent {
        try encodeStandard(value: .intent(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> Intent {
        try standardValue(encoded) {
            if case let .intent(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct ReplyCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .reply)
    }

    public func encode(_ value: ReplyContent) throws -> EncodedContent {
        try encodeStandard(value: .reply(reference: value.reference, referenceInboxId: value.referenceInboxId, content: value.content))
    }

    public func decode(_ encoded: EncodedContent) throws -> ReplyContent {
        try standardValue(encoded) {
            if case let .reply(reference, referenceInboxId, content) = $0 {
                ReplyContent(reference: reference, referenceInboxId: referenceInboxId, content: content)
            } else {
                nil
            }
        }
    }
}

public struct GroupUpdatedCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .groupUpdated)
    }

    public func encode(_ value: GroupUpdated) throws -> EncodedContent {
        try encodeStandard(value: .groupUpdated(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> GroupUpdated {
        try standardValue(encoded) {
            if case let .groupUpdated(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct DeleteMessageCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .deleteMessage)
    }

    public func encode(_ value: DeleteMessageContent) throws -> EncodedContent {
        try encodeStandard(value: .deleteMessage(messageId: value.messageId))
    }

    public func decode(_ encoded: EncodedContent) throws -> DeleteMessageContent {
        try standardValue(encoded) {
            if case let .deleteMessage(messageId) = $0 {
                DeleteMessageContent(messageId: messageId)
            } else {
                nil
            }
        }
    }
}

public struct LeaveRequestCodec: ContentCodec {
    public init() {}
    public var type: ContentTypeId {
        standardContentType(kind: .leaveRequest)
    }

    public func encode(_ value: LeaveRequest) throws -> EncodedContent {
        try encodeStandard(value: .leaveRequest(value))
    }

    public func decode(_ encoded: EncodedContent) throws -> LeaveRequest {
        try standardValue(encoded) {
            if case let .leaveRequest(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}
