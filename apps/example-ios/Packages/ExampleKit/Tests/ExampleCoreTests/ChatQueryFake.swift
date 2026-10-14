import ExampleCore

@MainActor final class ChatQueryFake: ChatQueries {
	var historyHook: (() async throws -> ChatPage)?
	var consent = ConversationConsent.allowed
	var mediaHook: (() async throws -> [ChatMessage])?
	var conversationHook: (() async throws -> Void)?
	var beforeRequests: [Int64?] = []
	var pendingHook: ((RecoveryPosition?) async throws -> PendingPage)?
	var publishHook: (() throws -> Void)?
	var deletes: [String] = []
	var deleteAttempts = 0
	var beforeDelete: () throws -> Void = {}
	var messageHook: ((String) async throws -> ChatMessage?)?
	var publishedIDs: [String] = []
	var publicationConversations: [String] = []
	var queues = 0; var publications = 0; var rows: [ChatMessage] = []; var beforeQueue: () throws
		-> Void = {}; var historyRead: (() async -> Void)?
	func conversation(id: String) async throws -> ConversationRecord {
		try await conversationHook?()
		return ConversationRecord(id: id, kind: .dm, consent: consent)
	}

	func history(id _: String, ascending: Bool, before: HistoryPosition?,
	             after: HistoryPosition?) async throws -> ChatPage
	{
		if let historyHook {
			return try await historyHook()
		}
		await historyRead?(); beforeRequests.append(before?.sentAt)
		var selected = rows
			.filter { row in (before.map { row.sentAt < $0.sentAt } ?? true) && (after.map { row.sentAt > $0.sentAt } ?? true) }
		if ascending {
			selected.reverse()
		}
		let page = Array(selected.prefix(50))
		return ChatPage(rows: page, first: page.first?.position, last: page.last?.position, hasMore: selected.count > 50)
	}

	func pending(id _: String, before: RecoveryPosition?) async throws -> PendingPage {
		if let pendingHook {
			return try await pendingHook(before)
		}
		return PendingPage(
			rows: [],
			last: nil,
			hasMore: false,
		)
	}

	func message(id: String) async throws -> ChatMessage? {
		if let messageHook {
			return try await messageHook(id)
		}
		return ChatMessage(
			id: id,
			conversationID: "chat",
			sender: "own",
			sentAt: 1,
			delivery: .queued,
			content: .text("hello"),
		)
	}

	func queue(conversationID _: String,
	           content _: OutboundContent) async throws -> String
	{
		try beforeQueue(); queues += 1; return "accepted"
	}

	func publish(conversationID: String, messageID: String) async throws {
		publications += 1; publishedIDs.append(messageID); publicationConversations.append(conversationID)
		try publishHook?()
	}

	func delete(messageID: String) async throws -> String {
		deleteAttempts += 1
		try beforeDelete()
		deletes.append(messageID)
		return "deletion-operation"
	}

	func media(conversationID _: String) async throws -> [ChatMessage] {
		try await mediaHook?() ?? []
	}
}
