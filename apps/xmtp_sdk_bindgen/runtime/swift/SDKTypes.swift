import Foundation

public enum SDKValueError: Error {
    case invalidID
    case clientClosed
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
        guard !value.isEmpty else { throw SDKValueError.invalidID }
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
        guard validHex(value, bytes: 32) else { throw SDKValueError.invalidID }
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
        guard validHex(value, bytes: 16) else { throw SDKValueError.invalidID }
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
        guard validHex(value, bytes: 32) else { throw SDKValueError.invalidID }
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

public final class Message: Identifiable, Hashable {
    public let data: MessageData
    public init(data: MessageData) {
        self.data = data
    }

    public var id: MessageID {
        data.id
    }

    public var conversationID: ConversationID {
        data.conversationID
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

    public var content: MessageContent {
        data.content
    }

    public func client() throws -> Client {
        guard let client = ClientRegistry.get(data.clientKey) else {
            throw SDKValueError.clientClosed
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
    weak var value: Client?
    init(_ value: Client) {
        self.value = value
    }
}

public enum ClientRegistry {
    private static let lock = NSLock()
    /// Every access to this map holds lock.
    private nonisolated(unsafe) static var entries: [UInt64: WeakClient] = [:]

    public static func register(_ client: Client) {
        lock.lock()
        defer { lock.unlock() }
        entries[client.clientKey()] = WeakClient(client)
    }

    public static func get(_ key: UInt64) -> Client? {
        lock.lock()
        defer { lock.unlock() }
        return entries[key]?.value
    }

    public static func remove(_ client: Client) {
        lock.lock()
        defer { lock.unlock() }
        entries.removeValue(forKey: client.clientKey())
    }
}
