import Foundation

public protocol SDKContentCodec: Sendable {
    var type: ContentTypeID { get }
    func encode(_ value: any Sendable) throws -> EncodedContent
    func decode(_ encoded: EncodedContent) throws -> any Sendable
}

public func SDKContentCodecKey(_ type: ContentTypeID) -> String {
    "\(type.authorityID)/\(type.typeID)/\(type.versionMajor)"
}

public extension SDKContentCodec {
    var key: String {
        SDKContentCodecKey(type)
    }
}

public enum SDKMessageContent: Sendable {
    case standard(MessageContent)
    case custom(encoded: EncodedContent, value: (any Sendable)?, error: Error?)
    case unknown(EncodedContent)
}

public enum SDKReplyContent: Sendable {
    case standard(MessageBody)
    case custom(encoded: EncodedContent, value: (any Sendable)?, error: Error?)
    case unknown(EncodedContent)
}

private func invalidID(_ message: String) -> XmtpError {
    .InvalidArgument(ErrorDetails(code: "InvalidArgument", category: .input, retryable: false, message: message))
}

private func clientClosedError() -> XmtpError {
    .ClientClosed(ErrorDetails(code: "ClientClosed", category: .lifecycle, retryable: false, message: "client is closed"))
}

private func validHex(_ value: String, bytes: Int) -> Bool {
    value.utf8.count == bytes * 2 && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
    }
}

public struct InboxID: Hashable, Sendable, CustomStringConvertible {
    public let value: String
    static func unchecked(_ value: String) -> Self {
        Self(value: value)
    }

    public static func fromString(_ value: String) throws -> Self {
        guard !value.isEmpty else { throw invalidID("inbox ID is empty") }
        return unchecked(value)
    }

    public var description: String {
        value
    }
}

public struct InstallationID: Hashable, Sendable, CustomStringConvertible {
    public let value: String
    static func unchecked(_ value: String) -> Self {
        Self(value: value)
    }

    public static func fromString(_ value: String) throws -> Self {
        guard validHex(value, bytes: 32) else { throw invalidID("invalid lowercase hex ID") }
        return unchecked(value)
    }

    public var description: String {
        value
    }
}

public struct ConversationID: Hashable, Sendable, CustomStringConvertible {
    public let value: String
    static func unchecked(_ value: String) -> Self {
        Self(value: value)
    }

    public static func fromString(_ value: String) throws -> Self {
        guard validHex(value, bytes: 16) else { throw invalidID("invalid lowercase hex ID") }
        return unchecked(value)
    }

    public var description: String {
        value
    }
}

public struct MessageID: Hashable, Sendable, CustomStringConvertible {
    public let value: String
    static func unchecked(_ value: String) -> Self {
        Self(value: value)
    }

    public static func fromString(_ value: String) throws -> Self {
        guard validHex(value, bytes: 32) else { throw invalidID("invalid lowercase hex ID") }
        return unchecked(value)
    }

    public var description: String {
        value
    }
}

public struct Timestamp: Hashable, Sendable {
    public let ns: Int64
    public var date: Date {
        Date(timeIntervalSince1970: Double(ns) / 1_000_000_000)
    }
}

private func decodeReplyBody(_ body: MessageBody, clientKey: UInt64) -> SDKReplyContent {
    switch body {
    case let .custom(encoded):
        let decoded = ClientRegistry.get(clientKey)?.decodeCustom(encoded)
            ?? .custom(encoded: encoded, value: nil, error: clientClosedError())
        switch decoded {
        case let .custom(_, value, error): return .custom(encoded: encoded, value: value, error: error)
        case .unknown: return .unknown(encoded)
        case .standard: return .standard(body)
        }
    case let .unknown(encoded): return .unknown(encoded)
    default: return .standard(body)
    }
}

public final class Message: Identifiable, Hashable, Sendable {
    public let data: MessageData
    public let content: SDKMessageContent
    public let inReplyToContent: SDKReplyContent?
    public let replyContent: SDKReplyContent?
    public init(data: MessageData) {
        self.data = data
        if case let .custom(encoded, _) = data.content {
            content = ClientRegistry.get(data.clientKey)?.decodeCustom(encoded)
                ?? .custom(encoded: encoded, value: nil, error: clientClosedError())
        } else {
            content = .standard(data.content)
        }
        inReplyToContent = data.inReplyTo.map { decodeReplyBody($0.content, clientKey: data.clientKey) }
        if case let .reply(_, body) = data.content {
            replyContent = decodeReplyBody(body, clientKey: data.clientKey)
        } else {
            replyContent = nil
        }
    }

    public var id: MessageID {
        data.id
    }

    public var conversationID: ConversationID {
        data.conversationID
    }

    public var topic: String {
        data.topic
    }

    public var senderInboxID: InboxID {
        data.senderInboxID
    }

    public var sentAt: Timestamp {
        data.sentAt
    }

    public var kind: MessageKind {
        data.kind
    }

    public var deliveryStatus: DeliveryStatus {
        data.deliveryStatus
    }

    public var contentType: ContentTypeID {
        data.contentType
    }

    public var fallback: String? {
        data.fallback
    }

    public var encoded: EncodedContent {
        data.encoded
    }

    public var replyCount: UInt64 {
        data.replyCount
    }

    public var reactions: [ReactionMessage] {
        data.reactions
    }

    public var inReplyTo: ReplyParent? {
        data.inReplyTo
    }

    public var insertedAt: Timestamp {
        data.insertedAt
    }

    public var expiresAt: Timestamp? {
        data.expiresAt
    }

    public func refresh() async throws -> Message? {
        try await client().raw.conversations().getMessageByID(id: id)
    }

    public func delete() async throws -> MessageID {
        try await client().raw.conversations().deleteMessage(id: id)
    }

    public func deleteLocally() async throws {
        try await client().raw.conversations().deleteMessageLocally(id: id)
    }

    public func react(_ reaction: Reaction, options: SendOptions? = nil) async throws -> MessageID {
        try await client().raw.conversations().reactToMessage(id: id, reaction: reaction, options: options)
    }

    public func reply(_ text: String, options: SendOptions? = nil) async throws -> MessageID {
        try await client().raw.conversations().replyToMessage(id: id, content: encodeText(text: text), options: options)
    }

    public func reply(_ content: EncodedContent, options: SendOptions? = nil) async throws -> MessageID {
        try await client().raw.conversations().replyToMessage(id: id, content: content, options: options)
    }

    public func reply(_ codec: any SDKContentCodec, value: any Sendable, options: SendOptions? = nil) async throws -> MessageID {
        try await reply(codec.encode(value), options: options)
    }

    public func parent() async throws -> Message? {
        guard let id = data.inReplyTo?.id else { return nil }
        return try await client().raw.conversations().getMessageByID(id: id)
    }

    public func conversation() async throws -> Conversation? {
        try await client().raw.conversations().getByID(id: conversationID)
    }

    public func client() throws -> SDKClient {
        guard let client = ClientRegistry.get(data.clientKey) else {
            throw clientClosedError()
        }
        return client
    }

    public static func == (lhs: Message, rhs: Message) -> Bool {
        lhs.id == rhs.id && lhs.data == rhs.data
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

private final class WeakClient {
    weak var value: SDKClient?
    init(_ value: SDKClient) {
        self.value = value
    }
}

public enum ClientRegistry {
    private static let lock = NSLock()
    /// Every access to this map holds lock.
    private nonisolated(unsafe) static var entries: [UInt64: WeakClient] = [:]

    public static func register(_ client: SDKClient) {
        lock.lock()
        defer { lock.unlock() }
        entries[client.raw.clientKey()] = WeakClient(client)
    }

    public static func get(_ key: UInt64) -> SDKClient? {
        lock.lock()
        defer { lock.unlock() }
        let value = entries[key]?.value
        if value == nil {
            entries.removeValue(forKey: key)
        }
        return value
    }

    public static func remove(_ client: SDKClient) {
        lock.lock()
        defer { lock.unlock() }
        entries.removeValue(forKey: client.raw.clientKey())
    }
}
