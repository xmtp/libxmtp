@testable import ExampleCore
import Foundation
import Testing

@MainActor struct ConversationRecoveryRoundFourteenTests {
	@Test(arguments: [true, false]) func consentRetryClearsPreviousDecisionError(accept: Bool) async throws {
		let queries = RecoveryConversationQueries(consent: .unknown)
		let preferences = RecoveryConversationPreferences()
		let fence = SessionFence(), routes = RouteOwner()
		let key = try #require(fence.replace(UUID()))
		let store = RequestsStore(session: key, fence: fence, routes: routes, queries: queries, preferences: preferences)
		await queries.failConsent(true)
		if accept {
			store.send(.accept("request"))
		} else {
			store.send(.requestBlock("request")); store.send(.confirm)
		}
		await store.waitForOperation()
		#expect(store.state.error != nil)
		await queries.failConsent(false)
		if accept {
			store.send(.accept("request"))
		} else {
			store.send(.requestBlock("request")); store.send(.confirm)
		}
		#expect(store.state.error == nil)
		await store.waitForOperation()
		#expect(store.state.error == nil)
		#expect(!store.state.busy)
		#expect(await queries.decisions == 2)
	}

	@Test func deleteRetryClearsPreviousDecisionError() throws {
		let queries = RecoveryConversationQueries(consent: .unknown)
		let preferences = RecoveryConversationPreferences()
		let fence = SessionFence(), routes = RouteOwner()
		let key = try #require(fence.replace(UUID()))
		let store = RequestsStore(session: key, fence: fence, routes: routes, queries: queries, preferences: preferences)
		preferences.failDismiss = true
		store.send(.requestDelete("request")); store.send(.confirm)
		#expect(store.state.error != nil)
		preferences.failDismiss = false
		store.send(.requestDelete("request"))
		#expect(store.state.error == nil)
		store.send(.confirm)
		#expect(store.state.error == nil)
		#expect(preferences.dismissedRequests == ["request"])
	}

	@Test(arguments: [ConversationConsent.allowed, .unknown])
	func appearingRowsRetainAllRequestedWindows(consent: ConversationConsent) async throws {
		let queries = RecoveryConversationQueries(consent: consent)
		let store = try await loadedStore(queries, consent: consent)
		store.send(.visible(start: 60, count: 1))
		store.send(.visible(start: 61, count: 1))
		store.send(.visible(start: 62, count: 1))
		await store.waitForLoad()
		#expect(store.state.rows[60].title == "Hydrated 60")
		#expect(store.state.rows[61].conversation.preview == "Preview 61")
		#expect(store.state.rows[62].conversation.unread == 63)
		#expect(await queries.detailReads.filter { $0 == "60" }.count == 1)
	}

	@Test func appearingRowDoesNotCancelEarlierInFlightDetails() async throws {
		let queries = RecoveryConversationQueries(consent: .allowed)
		let store = try await loadedStore(queries, consent: .allowed)
		await queries.holdDetails("60")
		store.send(.visible(start: 60, count: 1))
		while await !queries.detailsAreHeld {
			await Task.yield()
		}
		store.send(.visible(start: 61, count: 1))
		store.send(.visible(start: 62, count: 1))
		await queries.releaseDetails()
		await store.waitForLoad()
		#expect(store.state.rows[60].title == "Hydrated 60")
		#expect(store.state.rows[61].title == "Hydrated 61")
		#expect(store.state.rows[62].title == "Hydrated 62")
		#expect(await queries.detailReads.filter { $0 == "60" }.count == 1)
	}

	@Test func paginationRetainsEarlierRequestedRows() async throws {
		let queries = RecoveryConversationQueries(consent: .allowed, total: 150)
		let store = try await loadedStore(queries, consent: .allowed)
		store.send(.visible(start: 60, count: 1))
		store.send(.visible(start: 99, count: 1))
		await store.waitForLoad()
		#expect(store.state.rows.count == 150)
		#expect(store.state.rows[60].title == "Hydrated 60")
		#expect(store.state.rows[99].title == "Hydrated 99")
	}

	private func loadedStore(_ queries: RecoveryConversationQueries,
	                         consent: ConversationConsent) async throws -> ConversationsStore
	{
		let fence = SessionFence()
		let key = try #require(fence.replace(UUID()))
		let store = ConversationsStore(
			session: key,
			fence: fence,
			routes: RouteOwner(),
			queries: queries,
			preferences: RecoveryConversationPreferences(),
			ownInboxID: "own",
			consent: consent,
		)
		store.send(.load)
		await store.waitForLoad()
		store.send(.loadMore)
		await store.waitForLoad()
		#expect(store.state.rows.count == 100)
		return store
	}
}

private actor RecoveryConversationQueries: ConversationQueries {
	let consent: ConversationConsent
	let total: Int
	var failDecision = false
	var decisions = 0
	var detailReads: [String] = []
	var heldID: String?
	var held: CheckedContinuation<Void, Never>?
	var detailsAreHeld: Bool {
		held != nil
	}

	init(consent: ConversationConsent, total: Int = 100) {
		self.consent = consent; self.total = total
	}

	func failConsent(_ value: Bool) {
		failDecision = value
	}

	func holdDetails(_ id: String) {
		heldID = id
	}

	func releaseDetails() {
		held?.resume(); held = nil; heldID = nil
	}

	func list(consent requested: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		guard requested == consent else { return [] }
		return (0 ..< min(total, limit)).map { ConversationRecord(id: String($0), kind: .group, consent: consent) }
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		detailReads.append(record.id)
		if record.id == heldID {
			await withCheckedContinuation { held = $0 }
		}
		var enriched = record
		enriched.name = "Hydrated " + record.id
		enriched.preview = "Preview " + record.id
		enriched.unread = (Int(record.id) ?? 0) + 1
		return enriched
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {
		decisions += 1
		if failDecision {
			throw MetadataRuleError.changed
		}
	}
}

@MainActor private final class RecoveryConversationPreferences: ConversationPreferences {
	var failDismiss = false
	var dismissedRequests: Set<String> = []
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
		if failDismiss {
			throw MetadataRuleError.changed
		}
		dismissedRequests.insert(id)
	}
}
