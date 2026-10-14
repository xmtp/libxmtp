import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test(arguments: [false, true]) func stalePendingCompletionKeepsNewerPage(directRefresh: Bool) async throws {
		let queries = ChatQueryFake(); let gate = ReviewGate(); var first = true
		func page(_ id: String) -> PendingPage {
			PendingPage(
				rows: [
					ChatMessage(id: id, conversationID: "chat", sender: "own", sentAt: 1, delivery: .failed, content: .text(id)),
				],
				last: nil,
				hasMore: false,
			)
		}
		queries.pendingHook = { _ in
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
		let refresh: Task<Void, Never>? = directRefresh ? Task { await timeline.refresh() } : nil
		if !directRefresh {
			timeline.send(.newestPending)
		}
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}; try #require(gate.started)
		timeline.send(.olderPending); await timeline.wait(); #expect(timeline.state.pending.map(\.id) == ["new"])
		gate.release(); if let refresh {
			await refresh.value
		}; await timeline.close()
		#expect(timeline.state.pending.map(\.id) == ["new"])
	}

	@Test func discardedAttachmentLeavesNoDurableDraftAfterRouteExit() async throws {
		let fixture = AttachmentFixture(); let draft = try fixture.add(); let gate = ReviewGate(); var valid = true
		fixture.gateway.discardHook = { await gate.wait() }
		let store = AttachmentStore(
			gateway: fixture.gateway,
			drafts: fixture.drafts,
			coordinator: fixture.coordinator,
			conversationID: "chat",
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			admit: { _ in valid },
			allowed: { true },
		)
		store.send(.discard(draft.id))
		for _ in 0 ..< 10000 {
			if gate.started {
				break
			}; await Task.yield()
		}; try #require(gate.started)
		valid = false; gate.release(); await store.wait(); #expect(try fixture.drafts.all().isEmpty)
	}
}
