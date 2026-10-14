import Foundation

public struct HistoryPosition: Codable, Hashable, Sendable {
	public var sentAt: Int64
	public var cursor: String
	public init(sentAt: Int64, cursor: String) {
		self.sentAt = sentAt
		self.cursor = cursor
	}
}

public struct RecoveryPosition: Codable, Hashable, Sendable {
	public var sentAt: Int64
	public var id: String
	public init(sentAt: Int64, id: String) {
		self.sentAt = sentAt
		self.id = id
	}
}

public enum MessageDelivery: String, Codable, Sendable {
	case queued, published, failed
}

public enum ChatContent: Equatable, Sendable {
	case text(String), markdown(String), reply(quote: String, body: String), attachment(FileDescriptor), deleted,
	     unsupported(String), reaction
	public var text: String {
		switch self {
		case let .text(value), let .markdown(value),
		     let .unsupported(value): value
		case let .reply(_,
		                body): body
		case let .attachment(
			file,
		): file
			.filename ?? "File"
		case .deleted: "Message deleted"
		case .reaction: ""
		}
	}

	public var isDeleted: Bool {
		self == .deleted
	}
}

public struct ReactionRecord: Equatable, Sendable {
	public var sender: String
	public var emoji: String
	public var schema: String
	public var added: Bool
	public var sentAt: Int64
	public init(sender: String, emoji: String, schema: String = "unicode", added: Bool, sentAt: Int64) {
		self.sender = sender
		self.emoji = emoji
		self.schema = schema
		self.added = added
		self.sentAt = sentAt
	}
}

public struct ReactionBadgeModel: Equatable, Sendable, Identifiable {
	public var emoji: String
	public var senders: [String]
	public var mine: Bool
	public var id: String {
		emoji
	}

	public var count: Int {
		senders.count
	}
}

public struct ChatMessage: Identifiable, Equatable, Sendable {
	public var id: String
	public var conversationID: String
	public var sender: String
	public var sentAt: Int64
	public var insertedAt: Int64
	public var delivery: MessageDelivery
	public var content: ChatContent
	public var position: HistoryPosition?
	public var reactions: [
		ReactionRecord
	]
	public init(
		id: String,
		conversationID: String,
		sender: String,
		sentAt: Int64,
		insertedAt: Int64 = 0,
		delivery: MessageDelivery = .published,
		content: ChatContent,
		position: HistoryPosition? = nil,
		reactions: [ReactionRecord] = [],
	) {
		self.id = id
		self.conversationID = conversationID
		self.sender = sender
		self.sentAt = sentAt
		self
			.insertedAt = insertedAt
		self.delivery = delivery
		self.content = content
		self.position = position
		self
			.reactions = reactions
	}

	public var date: Date {
		Date(timeIntervalSince1970: Double(sentAt) / 1_000_000_000)
	}
}

public enum MessageMapper {
	public static func reactions(_ records: [ReactionRecord], own: String) -> [ReactionBadgeModel] {
		struct Key: Hashable { let sender: String
			let emoji: String
			let schema: String
		}
		var latest: [Key: ReactionRecord] = [:]
		for (_, record) in records.enumerated()
			.sorted(by: { $0.element.sentAt == $1.element.sentAt ? $0.offset < $1.offset : $0.element.sentAt < $1.element.sentAt
			})
		{
			latest[Key(sender: record.sender, emoji: record.emoji, schema: record.schema)] = record
		}
		let active = latest.values.filter(\.added)
		return Dictionary(grouping: active, by: { $0.emoji }).map { emoji, values in ReactionBadgeModel(
			emoji: emoji,
			senders: values.map(\.sender).sorted(),
			mine: values.contains { $0.sender == own },
		) }.sorted { $0.emoji < $1.emoji }
	}
}

public struct ChatPage: Sendable {
	public var rows: [
		ChatMessage
	]
	public var first: HistoryPosition?
	public var last: HistoryPosition?
	public var hasMore: Bool
	public var skipped: UInt32
	public init(rows: [ChatMessage], first: HistoryPosition?, last: HistoryPosition?, hasMore: Bool, skipped: UInt32 = 0) {
		self.rows = rows
		self.first = first
		self.last = last
		self.hasMore = hasMore
		self.skipped = skipped
	}
}

public struct PendingPage: Sendable {
	public var rows: [
		ChatMessage
	]
	public var last: RecoveryPosition?
	public var hasMore: Bool
	public var skipped: UInt32
	public init(rows: [ChatMessage], last: RecoveryPosition?, hasMore: Bool, skipped: UInt32 = 0) {
		self.rows = rows
		self.last = last
		self.hasMore = hasMore
		self.skipped = skipped
	}
}

public protocol ChatQueries: Sendable {
	func conversation(id: String) async throws -> ConversationRecord
	func history(id: String, ascending: Bool, before: HistoryPosition?, after: HistoryPosition?) async throws -> ChatPage
	func pending(id: String, before: RecoveryPosition?) async throws -> PendingPage
	func message(id: String) async throws -> ChatMessage?
	func queue(conversationID: String, content: OutboundContent) async throws -> String
	func publish(conversationID: String, messageID: String) async throws
	/// Queue a deletion and return its accepted operation ID; publish that ID to confirm it.
	func delete(messageID: String) async throws -> String
	func media(conversationID: String) async throws -> [ChatMessage]
}

public enum OutboundContent: Equatable, Sendable {
	case text(String), reply(parent: String, text: String), reaction(
		parent: String,
		emoji: String,
		remove: Bool,
	), attachment(FileDescriptor)
}

public enum ChatError: Error, Equatable, Sendable {
	case unavailable, rejected, stale, duplicate, storage, unknown
}
