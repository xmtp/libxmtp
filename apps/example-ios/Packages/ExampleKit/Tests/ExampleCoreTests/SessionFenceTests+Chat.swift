@testable import ExampleCore
import Foundation
import Testing

@MainActor @Test func chatDropsHistoryAfterDestinationChanges() async {
	var valid = true; let queries = ChatQueryFake()
	queries.rows = [ChatMessage(id: "late", conversationID: "chat", sender: "peer", sentAt: 1, content: .text("late"))]
	queries.historyRead = { valid = false }
	let store = ChatTimelineStore(
		id: "chat",
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		queries: queries,
		admit: { _ in valid },
	)
	store.send(.load); await store.wait()
	#expect(store.state.window.rows.isEmpty)
}

@MainActor @Test func returningFromSheetBindsFreshStores() async throws {
	let routes = RouteOwner(); let fence = SessionFence()
	let key = try #require(fence.replace(UUID()))
	routes.show(.chat("chat"))
	let first = try #require(ChatDestination(conversationID: "chat", session: key, routes: routes))
	var saved = ComposerDraft(); var records: [SendDraft] = []
	let queries = ChatQueryFake()
	queries.rows = [ChatMessage(id: "first", conversationID: "chat", sender: "peer", sentAt: 1, content: .text("first"))]
	let sends = SendDraftStore(read: { records }, write: { records = $0 })
	let drafts = ComposerDraftStore(read: { _ in saved }, write: { _, value in saved = value })
	let coordinator = SendCoordinator(drafts: sends, queries: queries, accepts: { true })
	func composer(_ destination: ChatDestination) -> ComposerStore {
		ComposerStore(
			id: "chat",
			origin: destination.origin,
			drafts: drafts,
			sends: sends,
			coordinator: coordinator,
			admit: { routes.admit($0, fence: fence) },
		)
	}
	let old = composer(first); old.send(.allow(true)); old.send(.edit("before"))
	routes.show(.emojiPicker(conversationID: "chat", messageID: "first"))
	#expect(ChatDestination(conversationID: "chat", session: key, routes: routes) == nil)
	routes.back()
	let returned = try #require(ChatDestination(conversationID: "chat", session: key, routes: routes))
	old.send(.edit("stale")); #expect(old.state.draft.text == "before")
	// SwiftUI's task identity comparison must recreate the stores after returning.
	let current = returned == first ? old : composer(returned)
	current.send(.allow(true)); current.send(.edit("after")); #expect(current.state.draft.text == "after")
	current.send(.submit); await current.wait(); #expect(queries.queues == 1)
	let timeline = ChatTimelineStore(
		id: "chat",
		origin: returned.origin,
		queries: queries,
		admit: { routes.admit($0, fence: fence) },
	)
	timeline.send(.load); await timeline.wait(); #expect(timeline.state.messages.count == 1)
}
