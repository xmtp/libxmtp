@testable import ExampleCore
import Foundation
import Testing

enum SearchMetadataField: Sendable { case title, handle, preview }
@MainActor struct ConversationSearchWindowTests {
	@Test(arguments: [SearchMetadataField.title, .handle, .preview])
	func searchBeforeListCompletesDiscoversFieldsInsideTheWindow(field: SearchMetadataField) async throws {
		let queries = SearchHydrationQueries(field: field, matchID: 25, holdPrimary: true)
		let store = try makeStore(queries)
		store.send(.load)
		await queries.waitForHeldPrimary()
		store.send(.search("NEEDLE"))
		await queries.releasePrimary()
		await store.waitForLoad()
		#expect(store.state.visibleRows.map(\.id) == ["25"])
		#expect(await queries.primaryLimits == [50])
		await store.close()
	}

	@Test func searchRestartsCanceledDetailsWithoutAFilteredRowAppearance() async throws {
		let queries = SearchHydrationQueries(field: .title, matchID: 25)
		await queries.holdDetails("25")
		let store = try makeStore(queries)
		store.send(.load)
		await queries.waitForHeldDetails()
		store.send(.search("needle"))
		await queries.releaseDetails()
		await store.waitForLoad()
		#expect(store.state.visibleRows.map(\.id) == ["25"])
		#expect(await queries.primaryLimits == [50])
		#expect(await queries.detailIDs.filter { $0 == "25" }.count == 2)
		await store.close()
	}

	@Test func searchDiscoveryKeepsTheExistingLoadedWindowBound() async throws {
		let queries = SearchHydrationQueries(field: .title, matchID: 80)
		let store = try makeStore(queries)
		store.send(.load); await store.waitForLoad()
		store.send(.loadMore); await store.waitForLoad()
		store.send(.search("needle")); await store.waitForLoad()
		store.send(.loadMore)
		#expect(await queries.detailIDs.allSatisfy { (Int($0) ?? 100) < 60 })
		#expect(await queries.primaryLimits == [50, 100])
		await store.close()
	}

	private func makeStore(_ queries: SearchHydrationQueries) throws -> ConversationsStore {
		let fence = SessionFence(), routes = RouteOwner()
		let key = try #require(fence.replace(UUID()))
		return ConversationsStore(session: key, fence: fence, routes: routes, queries: queries,
		                          preferences: SearchHydrationPreferences(), ownInboxID: "own")
	}
}

private actor SearchHydrationQueries: ConversationQueries {
	let field: SearchMetadataField
	let matchID: Int
	var holdPrimary: Bool
	var primaryLimits: [Int] = []
	var detailIDs: [String] = []
	private var primaryHeld: CheckedContinuation<Void, Never>?
	private var heldDetailID: String?
	private var detailHeld: CheckedContinuation<Void, Never>?
	private var deniedID: String?
	private var metadataEnabled = true
	init(field: SearchMetadataField, matchID: Int, holdPrimary: Bool = false) {
		self.field = field; self.matchID = matchID; self.holdPrimary = holdPrimary
	}

	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		guard consent == .allowed else { return [] }
		primaryLimits.append(limit)
		if holdPrimary {
			await withCheckedContinuation { primaryHeld = $0 }
		}
		return (0 ..< min(100, limit)).map { ConversationRecord(id: String($0), kind: field == .handle ? .dm : .group) }
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		detailIDs.append(record.id)
		if record.id == heldDetailID {
			heldDetailID = nil
			await withCheckedContinuation { detailHeld = $0 }
		}
		var result = record
		if metadataEnabled, record.id == String(matchID) {
			switch field {
			case .title: result.name = "Needle garden"
			case .handle: result.peerHandle = "needle#1234"
			case .preview: result.preview = "Find the needle here"
			}
		}
		if record.id == deniedID {
			result.consent = .denied
		}
		return result
	}

	func holdDetails(_ id: String) {
		heldDetailID = id
	}

	func waitForHeldDetails() async {
		while detailHeld == nil {
			await Task.yield()
		}
	}

	func releaseDetails() {
		detailHeld?.resume(); detailHeld = nil; heldDetailID = nil
	}

	func enableMetadata(_ enabled: Bool) {
		metadataEnabled = enabled
	}

	func denyDuringDetails(_ id: String) {
		deniedID = id
	}

	func waitForHeldPrimary() async {
		while primaryHeld == nil {
			await Task.yield()
		}
	}

	func releasePrimary() {
		holdPrimary = false; primaryHeld?.resume(); primaryHeld = nil
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@MainActor private final class SearchHydrationPreferences: ConversationPreferences {
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
		dismissedRequests.insert(id)
	}
}
