@testable import ExampleCore
import Foundation
import Testing

@MainActor struct RequestPrimaryReadTests {
	@Test func requestsHydrateWithoutASecondBlockedCountRead() async throws {
		let queries = RequestReadQueries(total: 120, blockCount: true, includeDenied: true)
		let store = try makeStore(queries, consent: .unknown)
		store.send(.load)
		await queries.waitForCountOrDetails()
		#expect(await queries.unknownLimits == [51])
		#expect(await queries.detailReads > 0)
		await queries.releaseCount()
		await store.waitForLoad()
		#expect(store.state.rows.count == 49)
		#expect(store.state.rows.allSatisfy { $0.conversation.consent == .unknown && $0.id != "0" })
		#expect(store.state.hasMore)
		#expect(store.state.requestCount == nil)
		store.send(.loadMore)
		await store.waitForLoad()
		#expect(await queries.unknownLimits == [51, 101])
		#expect(store.state.rows.count == 99)
		await store.close()
	}

	@Test func exhaustedRequestsNeedNoBadgeRead() async throws {
		let queries = RequestReadQueries(total: 30)
		let store = try makeStore(queries, consent: .unknown)
		store.send(.load); await store.waitForLoad()
		#expect(await queries.unknownLimits == [51])
		#expect(store.state.rows.count == 29)
		#expect(!store.state.hasMore)
		#expect(store.state.requestCount == nil)
		await store.close()
	}

	@Test(arguments: [200, 501]) func chatsStillFetchCappedUnloadedRequestCount(total: Int) async throws {
		let queries = RequestReadQueries(total: total)
		let store = try makeStore(queries, consent: .allowed)
		store.send(.load); await store.waitForLoad()
		#expect(await queries.unknownLimits == [501])
		#expect(store.state.requestCount == (total < 501 ? total - 1 : nil))
		await store.close()
	}

	private func makeStore(_ queries: RequestReadQueries, consent: ConversationConsent) throws -> ConversationsStore {
		let fence = SessionFence(), routes = RouteOwner()
		let key = try #require(fence.replace(UUID()))
		return ConversationsStore(session: key, fence: fence, routes: routes, queries: queries,
		                          preferences: RequestReadPreferences(), ownInboxID: "own", consent: consent)
	}
}

private actor RequestReadQueries: ConversationQueries {
	let total: Int
	let blockCount: Bool
	let includeDenied: Bool
	let limiter = AsyncLimiter()
	var unknownLimits: [Int] = []
	var detailReads = 0
	private var countHeld: CheckedContinuation<Void, Never>?
	private var countCanBlock = true
	init(total: Int, blockCount: Bool = false, includeDenied: Bool = false) {
		self.total = total; self.blockCount = blockCount; self.includeDenied = includeDenied
	}

	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		try await limiter.withPermit { await self.readList(consent: consent, limit: limit) }
	}

	private func readList(consent: ConversationConsent, limit: Int) async -> [ConversationRecord] {
		guard consent == .unknown else { return [] }
		unknownLimits.append(limit)
		if blockCount, countCanBlock, limit == 501 {
			await withCheckedContinuation { countHeld = $0 }
		}
		return (0 ..< min(total, limit)).map {
			ConversationRecord(id: String($0), kind: .group, consent: includeDenied && $0 == 1 ? .denied : .unknown)
		}
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		try await limiter.withPermit { await self.readDetails(record) }
	}

	private func readDetails(_ record: ConversationRecord) -> ConversationRecord {
		detailReads += 1; return record
	}

	func waitForCountOrDetails() async {
		let clock = ContinuousClock(), deadline = ContinuousClock.now + .seconds(2)
		while countHeld == nil, detailReads == 0, clock.now < deadline {
			await Task.yield()
		}
	}

	func releaseCount() {
		countCanBlock = false; countHeld?.resume(); countHeld = nil
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor private final class RequestReadPreferences: ConversationPreferences {
	var dismissedRequests: Set = ["0"]
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
