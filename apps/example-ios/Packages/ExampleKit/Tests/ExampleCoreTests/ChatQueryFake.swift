import ExampleCore

@MainActor final class ChatQueryFake: ChatQueries {
	var conversationHook: (() async throws -> Void)?
	var beforeRequests: [Int64?] = []
	var pendingHook: ((RecoveryPosition?) async throws -> PendingPage)?
	var deletes: [String] = []
	var queues = 0; var publications = 0; var rows: [ChatMessage] = []; var beforeQueue: () throws
		-> Void = {}; var historyRead: (() async -> Void)?
	func conversation(id: String) async throws -> ConversationRecord {
		try await conversationHook?()
		return ConversationRecord(id: id, kind: .dm)
	}

	func history(id _: String, ascending: Bool, before: HistoryPosition?,
	             after: HistoryPosition?) async throws -> ChatPage
	{
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
		ChatMessage(
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

	func publish(conversationID _: String, messageID _: String) async throws {
		publications += 1
	}

	func delete(messageID: String) async throws {
		deletes.append(messageID)
	}

	func media(conversationID _: String) async throws -> [ChatMessage] {
		[]
	}
}
