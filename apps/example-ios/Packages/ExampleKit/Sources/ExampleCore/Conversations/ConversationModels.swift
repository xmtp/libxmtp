import Foundation

public enum ConversationConsent: String, Sendable, Codable { case allowed, unknown, denied }
public enum ConversationKind: String, Sendable, Codable { case group, dm }
public struct ConversationRecord: Identifiable, Equatable, Sendable {
	public var id: String
	public var kind: ConversationKind
	public var name: String
	public var peerInboxID: String?
	public var peerHandle: String?
	public var consent: ConversationConsent
	public var preview: String
	public var unread: Int
	public var lastActivity: Date?
	public var memberCount: Int?
	public init(
		id: String,
		kind: ConversationKind,
		name: String = "",
		peerInboxID: String? = nil,
		peerHandle: String? = nil,
		consent: ConversationConsent = .allowed,
		preview: String = "",
		unread: Int = 0,
		lastActivity: Date? = nil,
		memberCount: Int? = nil,
	) {
		self.id = id; self.kind = kind; self.name = name; self.peerInboxID = peerInboxID; self.peerHandle = peerHandle; self
			.consent = consent; self.preview = preview; self.unread = unread
		self.lastActivity = lastActivity
		self.memberCount = memberCount
	}

	public func logicalKey(ownInboxID: String) -> String {
		kind == .group ? id : "dm:" + [ownInboxID, peerInboxID ?? ""].sorted().joined(separator: ":")
	}

	public func title(alias: String? = nil) -> String {
		if let alias, !alias.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
			return alias
		}
		if kind == .group {
			return name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Group" : name
		}
		if let peerHandle, !peerHandle.isEmpty {
			return "@" + peerHandle
		}
		if let peerInboxID, !peerInboxID.isEmpty {
			return String(peerInboxID.prefix(12))
		}
		return "Direct message"
	}
}

public struct ConversationRow: Identifiable, Equatable, Sendable {
	public var conversation: ConversationRecord
	public var title: String
	public var muted: Bool
	public init(conversation: ConversationRecord, title: String, muted: Bool) {
		self.conversation = conversation; self.title = title; self.muted = muted
	}

	public var id: String {
		conversation.id
	}

	public var initials: String {
		switch conversation.kind {
		case .group:
			DisplayInitials.group(title)
		case .dm:
			DisplayInitials.person(title, isInboxID: conversation.peerInboxID.map { title == String($0.prefix(12)) } ?? false)
		}
	}

	public func matches(_ search: String) -> Bool {
		[title, conversation.peerHandle ?? "", conversation.preview, id]
			.contains { $0.localizedCaseInsensitiveContains(search) }
	}
}

public enum ListRules {
	public static func detailWindow(start: Int, count: Int, total: Int) -> Range<Int> {
		let lower = min(max(0, start), total); return lower ..< min(total, lower + max(0, count) + 50)
	}

	public static func shouldPage(lastVisible: Int, loaded: Int, search: String) -> Bool {
		search.isEmpty && lastVisible >= loaded - 3
	}

	public static func requestCount(_ fetched: Int) -> Int? {
		fetched < 501 ? fetched : nil
	}
}

public enum UnreadContent: Sendable { case text, markdown, reply, remoteAttachment, other }
public enum UnreadRules {
	// Keep the six independent unread admission inputs explicit at call sites.
	// swiftlint:disable:next function_parameter_count
	public static func includes(
		content: UnreadContent,
		sender: String,
		own: String,
		published: Bool,
		insertedAt: Int64,
		marker: Int64,
	) -> Bool {
		published && sender != own && insertedAt > marker && content != .other
	}

	public static func canMarkRead(foreground: Bool, currentChat: Bool, newestPage: Bool, atNewest: Bool) -> Bool {
		foreground && currentChat && newestPage && atNewest
	}
}

public protocol ConversationQueries: Sendable {
	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord]
	func details(_ record: ConversationRecord, marker: Int64) async throws -> ConversationRecord
	func setConsent(id: String, consent: ConversationConsent) async throws
}

@MainActor public protocol ConversationPreferences: AnyObject {
	func alias(for id: String) -> String?
	func muted(for logicalKey: String) -> Bool
	func marker(for logicalKey: String) -> Int64
	var dismissedRequests: Set<String> { get }
	func dismissRequest(_ id: String) throws
}
