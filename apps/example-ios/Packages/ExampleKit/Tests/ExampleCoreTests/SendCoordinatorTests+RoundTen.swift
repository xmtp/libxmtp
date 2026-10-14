import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test func pendingMessagesAreChronological() async {
		let queries = ChatQueryFake()
		queries.pendingHook = { _ in PendingPage(rows: [
			ChatMessage(id: "new", conversationID: "chat", sender: "own", sentAt: 2, delivery: .failed, content: .text("new")),
			ChatMessage(id: "old", conversationID: "chat", sender: "own", sentAt: 1, delivery: .failed, content: .text("old")),
		], last: nil, hasMore: false) }
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		timeline.send(.newestPending); await timeline.wait()
		#expect(timeline.state.messages.map(\.id) == ["old", "new"])
		await timeline.close()
	}

	@Test func staleHistoryCannotReplaceDirectRefresh() async throws {
		let queries = ChatQueryFake(); let gate = ReviewGate(); var first = true
		func page(_ id: String) -> ChatPage {
			let row = ChatMessage(
				id: id,
				conversationID: "chat",
				sender: "peer",
				sentAt: 1,
				delivery: .published,
				content: .text(id),
			)
			return ChatPage(rows: [row], first: row.position, last: row.position, hasMore: false)
		}
		queries.historyHook = {
			if first {
				first = false; await gate.wait(); return page("old")
			}
			return page("new")
		}
		let timeline = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			queries: queries,
			admit: { _ in true },
		)
		// Direct refreshes have no owning job in the original implementation.
		let slow = Task { await timeline.refresh() }
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}
		try #require(gate.started)
		await timeline.refresh(); #expect(timeline.state.window.rows.map(\.id) == ["new"])
		gate.release(); await slow.value; await timeline.close()
		#expect(timeline.state.window.rows.map(\.id) == ["new"])
	}
}
