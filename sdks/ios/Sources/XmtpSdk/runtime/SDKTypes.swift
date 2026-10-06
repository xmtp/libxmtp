import Foundation

/// A content codec with a typed value (Ref Public surface, Host codecs). The
/// send helpers run its steps before the send: `encode`, then `fallback` when
/// the envelope has none, then `shouldPush` when the send has no explicit
/// `shouldPush` option and the type is not a catalogue type. A step that
/// throws fails the send with `XmtpError.CodecEncodeFailed`, and the SDK makes
/// no publish attempt.
public protocol ContentCodec: Sendable {
    associatedtype Value: Sendable
    var type: ContentTypeId { get }
    func encode(_ value: Value) throws -> EncodedContent
    func decode(_ encoded: EncodedContent) throws -> Value
    /// Text for recipients without this codec. Default: no fallback.
    func fallback(_ value: Value) throws -> String?
    /// Whether sending this value notifies recipients. Default: it does.
    func shouldPush(_ value: Value) throws -> Bool
}

public extension ContentCodec {
    func fallback(_: Value) throws -> String? {
        nil
    }

    func shouldPush(_: Value) throws -> Bool {
        true
    }
}

/// The registry key of a content type: authority, type ID, and major version.
struct ContentCodecKey: Hashable {
    let authorityId: String
    let typeId: String
    let versionMajor: UInt32

    init(_ type: ContentTypeId) {
        authorityId = type.authorityId
        typeId = type.typeId
        versionMajor = type.versionMajor
    }
}

public enum SDKMessageContent: Sendable {
    case standard(MessageContent)
    case custom(encoded: EncodedContent, rawBytes: Data, value: (any Sendable)?, error: ErrorDetails?)
    case unknown(encoded: EncodedContent?, rawBytes: Data, error: ErrorDetails)
}

public enum SDKReplyContent: Sendable {
    case standard(MessageBody)
    case custom(encoded: EncodedContent, rawBytes: Data, value: (any Sendable)?, error: ErrorDetails?)
    case unknown(encoded: EncodedContent?, rawBytes: Data, error: ErrorDetails)
}

private func clientClosedError() -> XmtpError {
    .ClientClosed(ErrorDetails(code: "ClientClosed", category: .lifecycle, retryable: false, message: "client is closed"))
}

public struct Timestamp: Hashable, Sendable {
    public let ns: Int64
    public init(ns: Int64) {
        self.ns = ns
    }

    public var date: Date {
        Date(timeIntervalSince1970: Double(ns) / 1_000_000_000)
    }
}

private func decodeReplyBody(_ body: MessageBody, clientKey: UInt64) -> SDKReplyContent {
    switch body {
    case let .custom(encoded, rawBytes):
        let decoded = ClientRegistry.get(clientKey)?.decodeCustom(encoded, rawBytes: rawBytes)
            ?? .custom(encoded: encoded, rawBytes: rawBytes, value: nil, error: closedContentDetails())
        switch decoded {
        case let .custom(_, _, value, error): return .custom(encoded: encoded, rawBytes: rawBytes, value: value, error: error)
        case let .unknown(encoded, rawBytes, error): return .unknown(encoded: encoded, rawBytes: rawBytes, error: error)
        case .standard: return .standard(body)
        }
    case let .unknown(encoded, rawBytes, error): return .unknown(encoded: encoded, rawBytes: rawBytes, error: error)
    default: return .standard(body)
    }
}

private func closedContentDetails() -> ErrorDetails {
    ErrorDetails(code: "ClientClosed", category: .lifecycle, retryable: false, message: "client is closed")
}

/// A received or stored message. `MessageFields.swift` holds its field
/// accessors.
public final class Message: Identifiable, Hashable, Sendable {
    public let data: MessageData
    public let content: SDKMessageContent
    public let inReplyToContent: SDKReplyContent?
    public let replyContent: SDKReplyContent?
    public init(data: MessageData) {
        self.data = data
        inReplyToContent = data.inReplyTo.map { decodeReplyBody($0.content, clientKey: data.clientKey) }
        if case let .reply(_, body) = data.content {
            replyContent = decodeReplyBody(body, clientKey: data.clientKey)
        } else {
            replyContent = nil
        }
        if case let .custom(_, _, _, error?)? = replyContent, error.code == "CodecDecodeFailed" {
            content = .unknown(encoded: data.encoded, rawBytes: data.rawBytes, error: error)
        } else if case let .custom(encoded, rawBytes) = data.content {
            content = ClientRegistry.get(data.clientKey)?.decodeCustom(encoded, rawBytes: rawBytes)
                ?? .custom(encoded: encoded, rawBytes: rawBytes, value: nil, error: closedContentDetails())
        } else if case let .unknown(encoded, rawBytes, error) = data.content {
            content = .unknown(encoded: encoded, rawBytes: rawBytes, error: error)
        } else {
            content = .standard(data.content)
        }
    }

    public func refresh() async throws -> Message? {
        try await client().raw.conversations().getMessageById(id: id)
    }

    public func delete() async throws -> MessageId {
        try await client().raw.conversations().deleteMessage(id: id)
    }

    public func deleteLocally() async throws {
        try await client().raw.conversations().deleteMessageLocally(id: id)
    }

    public func react(_ reaction: Reaction, options: SendOptions? = nil) async throws -> MessageId {
        try await client().raw.conversations().reactToMessage(id: id, reaction: reaction, options: options)
    }

    public func reply(_ text: String, options: SendOptions? = nil) async throws -> MessageId {
        try await client().raw.conversations().replyToMessage(id: id, content: encodeText(text: text), options: options)
    }

    public func reply(_ content: EncodedContent, options: SendOptions? = nil) async throws -> MessageId {
        try await client().raw.conversations().replyToMessage(id: id, content: content, options: options)
    }

    /// Reply with a value of a typed codec. The codec's fallback applies to the
    /// nested envelope; the reply keeps the reply type's push default unless
    /// `options.shouldPush` is set. A failed codec step is `CodecEncodeFailed`,
    /// with no publish attempt.
    public func reply<C: ContentCodec>(_ codec: C, value: C.Value, options: SendOptions? = nil) async throws -> MessageId {
        let encoded = try encodeForSend(codec, value: value)
        // A task cancelled during a codec step stops before the reply is sent.
        try Task.checkCancellation()
        return try await reply(encoded, options: options)
    }

    public func parent() async throws -> Message? {
        guard let id = data.inReplyTo?.id else { return nil }
        return try await client().raw.conversations().getMessageById(id: id)
    }

    public func conversation() async throws -> Conversation? {
        try await client().raw.conversations().getById(id: conversationId)
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

enum ClientRegistry {
    private static let lock = NSLock()
    /// Every access to this map holds lock.
    private nonisolated(unsafe) static var entries: [UInt64: WeakClient] = [:]

    static func register(_ client: SDKClient) {
        lock.lock()
        defer { lock.unlock() }
        entries[client.raw.clientKey()] = WeakClient(client)
    }

    static func get(_ key: UInt64) -> SDKClient? {
        lock.lock()
        defer { lock.unlock() }
        let value = entries[key]?.value
        if value == nil {
            entries.removeValue(forKey: key)
        }
        return value
    }

    static func remove(_ client: SDKClient) {
        lock.lock()
        defer { lock.unlock() }
        entries.removeValue(forKey: client.raw.clientKey())
    }
}
