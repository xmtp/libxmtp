@testable import ExampleCore
import Foundation
import Testing

struct MessageMapperTests {
	@Test func lastReactionPerSenderEmojiAndSchemaWins() {
		let records = [
			ReactionRecord(sender: "own", emoji: "👍", added: true, sentAt: 1),
			ReactionRecord(sender: "own", emoji: "👍", added: false, sentAt: 2),
			ReactionRecord(sender: "peer", emoji: "👍", added: true, sentAt: 3),
			ReactionRecord(sender: "peer", emoji: "👍", added: false, sentAt: 3),
			ReactionRecord(sender: "other", emoji: "❤️", added: true, sentAt: 4),
		]
		let result = MessageMapper.reactions(records, own: "own")
		#expect(result.count == 1); #expect(result.first?.emoji == "❤️"); #expect(result.first?.count == 1); #expect(result
			.first?.mine == false)
	}

	@Test func removedAnchorUsesNearestSurvivingNeighbor() {
		let anchor = ScrollAnchor(messageID: "gone", position: HistoryPosition(sentAt: 10, cursor: "cursor"))
		let rows = [
			ChatMessage(id: "older", conversationID: "chat", sender: "peer", sentAt: 8, content: .text("old")),
			ChatMessage(id: "newer", conversationID: "chat", sender: "peer", sentAt: 12, content: .text("new")),
		]
		let target = AnchorRestore.target(anchor, rows: rows)
		#expect(target.id == "newer"); #expect(target.changed)
	}
}

@MainActor private func longHistory() -> ChatQueryFake {
	let queries = ChatQueryFake()
	queries.rows = (1 ... 1200).reversed().map { number in ChatMessage(
		id: String(number),
		conversationID: "chat",
		sender: "peer",
		sentAt: Int64(number),
		content: .text("message"),
		position: HistoryPosition(sentAt: Int64(number), cursor: String(number)),
	) }
	return queries
}

extension MessageMapperTests {
	@MainActor @Test func olderPagesProgressBeyondTheCacheLimit() async {
		let queries = longHistory()
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		timeline.send(.load); await timeline.wait()
		for _ in 0 ..< 13 {
			timeline.send(.older); await timeline.wait()
		}
		#expect(timeline.state.window.rows.count == 500)
		#expect(timeline.state.window.last?.sentAt == 501)
		#expect(queries.beforeRequests.compactMap(\.self).suffix(3) == [651, 601, 551])
	}

	@MainActor @Test func streamRefreshPreservesLoadedHistory() async {
		let queries = longHistory()
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		timeline.send(.load); await timeline.wait()
		for _ in 0 ..< 13 {
			timeline.send(.older); await timeline.wait()
		}
		let ids = timeline.state.window.rows.map(\.id)
		await timeline.refresh()
		#expect(timeline.state.window.rows.map(\.id) == ids)
	}

	@MainActor @Test func visibleOlderRowsPersistTheReadingAnchor() async {
		let queries = longHistory(); var saved: ScrollAnchor?
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
			saveAnchor: { saved = $0 },
		)
		timeline.send(.load); await timeline.wait()
		timeline.send(.visibleMessages(["1180", "1181", "1182"], foreground: true))
		#expect(saved?.messageID == "1180"); #expect(timeline.state.anchorID == "1180"); #expect(!timeline.state.atNewest)
	}
}

extension MessageMapperTests {
	@Test @MainActor func refreshedSuffixKeepsEndOfHistory() {
		let cache = TranscriptCache(); var previous = HistoryWindow()
		previous.rows = [3, 2, 1].map { value in ChatMessage(
			id: String(value),
			conversationID: "chat",
			sender: "peer",
			sentAt: Int64(value),
			content: .text("message"),
			position: HistoryPosition(sentAt: Int64(value), cursor: String(value)),
		) }
		previous.first = previous.rows.first?.position; previous.last = previous.rows.last?.position; previous
			.hasOlder = false
		_ = cache.put("chat", window: previous)
		var page = previous; page.rows = Array(previous.rows.prefix(2)); page.last = page.rows.last?.position; page
			.hasOlder = true
		let refreshed = cache.refresh("chat", page: page, anchor: "1")
		#expect(refreshed.rows.map(\.id) == ["3", "2", "1"]); #expect(!refreshed.hasOlder); #expect(refreshed.last == previous
			.last)
	}
}
