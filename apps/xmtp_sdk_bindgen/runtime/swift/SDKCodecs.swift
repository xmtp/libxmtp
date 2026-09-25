import Foundation

private func codecValueError() -> XmtpError {
    .InvalidArgument(ErrorDetails(code: "InvalidArgument", category: .input, retryable: false, message: "wrong standard codec value"))
}

private func encodePure<T>(_ value: Any, as _: T.Type, wrap: (T) -> StandardContent) throws -> EncodedContent {
    guard let value = value as? T else { throw codecValueError() }
    return try encodeStandard(value: wrap(value))
}

private func decodePure<T>(_ encoded: EncodedContent, take: (StandardContent) -> T?) throws -> Any {
    guard let value = try take(decodeStandard(encoded: encoded)) else { throw codecValueError() }
    return value
}

public struct TextCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .text)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: String.self, wrap: StandardContent.text)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .text(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct ReadReceiptCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .readReceipt)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        guard value is Void else { throw codecValueError() }
        return try encodeStandard(value: .readReceipt)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case .readReceipt = $0 {
                ()
            } else {
                nil
            }
        }
    }
}

public struct ReactionV2Codec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .reaction)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        guard let content = value as? StandardContent, case .reaction = content else { throw codecValueError() }
        return try encodeStandard(value: content)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case .reaction = $0 {
                $0
            } else {
                nil
            }
        }
    }
}

public struct AttachmentCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .attachment)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: Attachment.self, wrap: StandardContent.attachment)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .attachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct RemoteAttachmentCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .remoteAttachment)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: RemoteAttachment.self, wrap: StandardContent.remoteAttachment)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .remoteAttachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct MultiRemoteAttachmentCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .multiRemoteAttachment)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: MultiRemoteAttachment.self, wrap: StandardContent.multiRemoteAttachment)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .multiRemoteAttachment(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct TransactionReferenceCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .transactionReference)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: TransactionReference.self, wrap: StandardContent.transactionReference)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .transactionReference(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct ReplyCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .reply)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        guard let content = value as? StandardContent, case .reply = content else { throw codecValueError() }
        return try encodeStandard(value: content)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case .reply = $0 {
                $0
            } else {
                nil
            }
        }
    }
}

public struct GroupUpdatedCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .groupUpdated)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: GroupUpdated.self, wrap: StandardContent.groupUpdated)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .groupUpdated(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}

public struct DeleteMessageCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .deleteMessage)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        guard let content = value as? StandardContent, case .deleteMessage = content else { throw codecValueError() }
        return try encodeStandard(value: content)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case .deleteMessage = $0 {
                $0
            } else {
                nil
            }
        }
    }
}

public struct LeaveRequestCodec: SDKContentCodec {
    public init() {}
    public var type: ContentTypeID {
        standardContentType(kind: .leaveRequest)
    }

    public func encode(_ value: Any) throws -> EncodedContent {
        try encodePure(value, as: LeaveRequest.self, wrap: StandardContent.leaveRequest)
    }

    public func decode(_ encoded: EncodedContent) throws -> Any {
        try decodePure(encoded) {
            if case let .leaveRequest(value) = $0 {
                value
            } else {
                nil
            }
        }
    }
}
