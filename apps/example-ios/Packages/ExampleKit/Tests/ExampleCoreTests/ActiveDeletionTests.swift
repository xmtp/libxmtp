@testable import ExampleCore
import Foundation
import Testing

struct ActiveDeletionTests {
	private struct Fixture {
		let store: ChatTimelineStore
		let queries: ChatQueryFake
		let cache: TranscriptCache
		let key: SessionKey
	}

	@MainActor private func fixture() -> Fixture {
		let queries = ChatQueryFake()
		queries.rows = [
			ChatMessage(id: "deleted", conversationID: "chat", sender: "peer", sentAt: 2, content: .reply(
				quote: "private quote",
				body: "private body",
			), position: HistoryPosition(sentAt: 2, cursor: "deleted"), reactions: [ReactionRecord(
				sender: "peer",
				emoji: "x",
				added: true,
				sentAt: 2,
			)]),
			ChatMessage(
				id: "kept",
				conversationID: "chat",
				sender: "peer",
				sentAt: 1,
				content: .text("kept"),
				position: HistoryPosition(sentAt: 1, cursor: "kept"),
			),
		]
		let cache = TranscriptCache()
		let key = SessionKey(profileID: UUID(), generation: 1)
		let store = ChatTimelineStore(
			id: "chat",
			origin: DestinationOrigin(session: key, token: 1),
			queries: queries,
			cache: cache,
			admit: { _ in true },
		)
		return Fixture(store: store, queries: queries, cache: cache, key: key)
	}

	@Test @MainActor func deletionImmediatelyRedactsPublishedAndPendingDespiteFailedRefresh() async {
		let fixture = fixture()
		let store = fixture.store; let queries = fixture.queries; let cache = fixture.cache; let key = fixture.key
		let pending = ChatMessage(
			id: "pending",
			conversationID: "chat",
			sender: "peer",
			sentAt: 3,
			delivery: .queued,
			content: .reply(quote: "private pending quote", body: "private pending body"),
			reactions: [ReactionRecord(sender: "peer", emoji: "x", added: true, sentAt: 3)],
		)
		queries.pendingHook = { _ in PendingPage(rows: [pending], last: nil, hasMore: false) }
		store.send(.load); await store.wait()
		store.send(.visible("deleted", newest: false, foreground: false))
		let before = store.state
		store.send(.messageDeleted(conversationID: "chat", messageID: "deleted", session: key))
		store.send(.messageDeleted(conversationID: "chat", messageID: "pending", session: key))
		#expect(store.state.window.rows[0].content == .deleted)
		#expect(store.state.window.rows[0].reactions.isEmpty)
		#expect(store.state.pending[0].content == .deleted)
		#expect(store.state.pending[0].reactions.isEmpty)
		#expect(store.state.window.rows[1] == before.window.rows[1])
		#expect(store.state.window.first == before.window.first && store.state.window.last == before.window.last)
		#expect(store.state.anchorID == before.anchorID && store.state.atNewest == before.atNewest)
		queries.conversationHook = { throw ChatError.unavailable }
		await store.refresh()
		#expect(store.state.window.rows[0].content == .deleted)
		#expect(store.state.pending[0].content == .deleted)
		#expect(cache.get("chat")?.rows[0].content == .deleted)
		await store.close()
	}

	@Test @MainActor func deletionFencesHeldPreEventRead() async {
		let fixture = fixture()
		let store = fixture.store; let queries = fixture.queries; let cache = fixture.cache; let key = fixture.key
		store.send(.load); await store.wait()
		var release: CheckedContinuation<Void, Never>?
		var entered: CheckedContinuation<Void, Never>?
		queries.conversationHook = {
			await withCheckedContinuation { continuation in
				release = continuation; entered?.resume(); entered = nil
			}
		}
		await withCheckedContinuation { continuation in
			entered = continuation; store.send(.refresh)
		}
		store.send(.messageDeleted(conversationID: "chat", messageID: "deleted", session: key))
		#expect(store.state.window.rows[0].content == .deleted)
		release?.resume(); await store.wait()
		#expect(store.state.window.rows[0].content == .deleted)
		#expect(store.state.window.rows[0].reactions.isEmpty)
		#expect(cache.get("chat")?.rows[0].content == .deleted)
		await store.close()
	}

	@Test @MainActor func deletionCallbackRejectsOtherSessionAndConversation() async {
		let fixture = fixture()
		let store = fixture.store; let key = fixture.key
		store.send(.load); await store.wait()
		let original = store.state
		store.send(.messageDeleted(conversationID: "other", messageID: "deleted", session: key))
		#expect(store.state == original)
		store.send(.messageDeleted(
			conversationID: "chat",
			messageID: "deleted",
			session: SessionKey(profileID: key.profileID, generation: 2),
		))
		#expect(store.state == original)
		await store.close()
	}
}
