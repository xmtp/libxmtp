@testable import ExampleCore
import Foundation
import Testing

@MainActor struct DismissedRequestPaginationTests {
	@Test(arguments: [50, 100]) func dismissedPrefixCannotHideOlderRequests(dismissed: Int) async throws {
		let queries = RequestPrefixQueries(total: dismissed + 75)
		let preferences = RequestPrefixPreferences(dismissed: dismissed)
		let fence = SessionFence()
		let key = try #require(fence.replace(UUID()))
		let store = ConversationsStore(session: key, fence: fence, routes: RouteOwner(), queries: queries,
		                               preferences: preferences, ownInboxID: "own", consent: .unknown)
		store.send(.load)
		await store.waitForLoad()
		#expect(store.state.rows.count == 50)
		#expect(store.state.rows.first?.id == String(dismissed))
		#expect(store.state.hasMore)
		store.send(.loadMore)
		await store.waitForLoad()
		#expect(store.state.rows.count == 75)
		#expect(store.state.rows.last?.id == String(dismissed + 74))
		#expect(!store.state.hasMore)
		await store.close()
	}

	@Test func exhaustedDismissedSourceHasNoMoreRows() async throws {
		let queries = RequestPrefixQueries(total: 50)
		let preferences = RequestPrefixPreferences(dismissed: 50)
		let fence = SessionFence(), key = try #require(fence.replace(UUID()))
		let store = ConversationsStore(session: key, fence: fence, routes: RouteOwner(), queries: queries,
		                               preferences: preferences, ownInboxID: "own", consent: .unknown)
		store.send(.load); await store.waitForLoad()
		#expect(store.state.rows.isEmpty)
		#expect(!store.state.hasMore)
		await store.close()
	}

	@Test func unrelatedDismissalsDoNotExpandDisplayedWindow() async throws {
		let queries = RequestPrefixQueries(total: 75)
		let preferences = RequestPrefixPreferences(dismissed: 0)
		preferences.dismissedRequests = Set((100 ..< 150).map(String.init))
		let fence = SessionFence(), key = try #require(fence.replace(UUID()))
		let store = ConversationsStore(session: key, fence: fence, routes: RouteOwner(), queries: queries,
		                               preferences: preferences, ownInboxID: "own", consent: .unknown)
		store.send(.load); await store.waitForLoad()
		#expect(store.state.rows.count == 50)
		#expect(store.state.hasMore)
		store.send(.loadMore); await store.waitForLoad()
		#expect(store.state.rows.count == 75)
		#expect(!store.state.hasMore)
		await store.close()
	}
}

private actor RequestPrefixQueries: ConversationQueries {
	let total: Int
	init(total: Int) {
		self.total = total
	}

	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		guard consent == .unknown else { return [] }
		return (0 ..< min(total, limit)).map { ConversationRecord(id: String($0), kind: .group, consent: .unknown) }
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor private final class RequestPrefixPreferences: ConversationPreferences {
	var dismissedRequests: Set<String>
	init(dismissed: Int) {
		dismissedRequests = Set((0 ..< dismissed).map(String.init))
	}

	func alias(for _: String) -> String? {
		nil
	}

	func muted(for _: String) -> Bool {
		false
	}

	func marker(for _: String) -> Int64 {
		0
	}

	func dismissRequest(_ id: String) throws {
		dismissedRequests.insert(id)
	}
}
