@testable import ExampleCore
import Foundation
import Testing

private actor DelayedConversations: ConversationQueries {
	var pending: CheckedContinuation<[ConversationRecord], Never>?
	func list(consent: ConversationConsent, limit _: Int) async throws -> [ConversationRecord] {
		if consent == .unknown {
			return []
		}
		return await withCheckedContinuation { pending = $0 }
	}

	func started() -> Bool {
		pending != nil
	}

	func finish() {
		pending?.resume(returning: [ConversationRecord(id: "late", kind: .group)]); pending = nil
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor private final class ConversationTestPreferences: ConversationPreferences {
	func alias(for _: String) -> String? {
		nil
	}

	func muted(for _: String) -> Bool {
		false
	}

	func marker(for _: String) -> Int64 {
		0
	}

	var dismissedRequests: Set<String> {
		[]
	}

	func dismissRequest(_: String) throws {}
}

@Test @MainActor func staleConversationPageIsRejected() async throws {
	let fence = SessionFence(), routes = RouteOwner(), query = DelayedConversations()
	let session = try #require(fence.replace(UUID()))
	let store = ConversationsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: query,
		preferences: ConversationTestPreferences(),
		ownInboxID: "own",
	)
	store.send(.load)
	while await !(query.started()) {
		await Task.yield()
	}
	_ = fence.reserve()
	await query.finish()
	await store.waitForLoad()
	#expect(store.state.rows.isEmpty)
}
