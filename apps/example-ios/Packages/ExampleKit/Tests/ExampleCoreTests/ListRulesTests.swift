@testable import ExampleCore
import Foundation
import Testing

@Test func conversationListRules() {
	#expect(ListRules.requestCount(500) == 500)
	#expect(ListRules.requestCount(501) == nil)
	#expect(ListRules.detailWindow(start: 10, count: 5, total: 100) == 10 ..< 65)
	#expect(ListRules.shouldPage(lastVisible: 47, loaded: 50, search: ""))
	#expect(!ListRules.shouldPage(lastVisible: 50, loaded: 50, search: "hello"))
	let dm = ConversationRecord(
		id: "abc",
		kind: .dm,
		peerInboxID: "123456789012345",
		peerHandle: "nick#1234",
		preview: "hello",
	)
	#expect(dm.title() == "@nick#1234")
	#expect(dm.title(alias: "Local name") == "Local name")
	#expect(ConversationRecord(id: "g", kind: .group).title() == "Group")
	#expect(ConversationRecord(id: "d", kind: .dm).title() == "Direct message")
	#expect(ConversationRecord(id: "d", kind: .dm, peerInboxID: "123456789012345").title() == "123456789012")
	let row = ConversationRow(conversation: dm, title: dm.title(), muted: false)
	#expect(row.initials == "N")
	for search in ["NICK", "HELLO", "ABC"] {
		#expect(row.matches(search))
	}
	#expect(!row.muted)
}

private actor VisibleWindowQueries: ConversationQueries {
	private var paused = false
	private var pending: CheckedContinuation<Void, Never>?
	private var generation = "Named"
	private var readIDs: [Int] = []
	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		guard consent == .allowed else { return [] }
		if paused {
			await withCheckedContinuation { pending = $0 }; paused = false
		}
		return (0 ..< min(limit, 100)).map { ConversationRecord(id: String($0), kind: .group) }
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		let id = try #require(Int(record.id))
		readIDs.append(id)
		var result = record
		result.name = generation + record.id
		return result
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
	func prepareRefresh() {
		generation = "Updated"; readIDs = []; paused = true
	}

	func waiting() -> Bool {
		pending != nil
	}

	func resume() {
		pending?.resume(); pending = nil
	}

	func reads() -> [Int] {
		readIDs
	}
}

@MainActor private final class WindowPreferences: ConversationPreferences {
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

@Test @MainActor func refreshPreservesVisibleDetailWindow() async throws {
	let fence = SessionFence(), routes = RouteOwner(), queries = VisibleWindowQueries()
	let session = try #require(fence.replace(UUID()))
	let store = ConversationsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: queries,
		preferences: WindowPreferences(),
		ownInboxID: "own",
	)
	store.send(.load)
	await store.waitForLoad()
	store.send(.loadMore)
	await store.waitForLoad()
	store.send(.visible(start: 70, count: 1))
	await store.waitForLoad()
	#expect(store.state.rows.first(where: { $0.id == "70" })?.title == "Named70")
	await queries.prepareRefresh()
	let refresh = Task { await store.refresh() }
	while await !(queries.waiting()) {
		await Task.yield()
	}
	#expect(store.state.rows.first(where: { $0.id == "70" })?.title == "Named70")
	await queries.resume()
	await refresh.value
	#expect(store.state.rows.first(where: { $0.id == "70" })?.title == "Updated70")
	let readIDs = await queries.reads()
	#expect(readIDs == Array(70 ..< 100))
	await store.close()
}

private actor DelayedConsentQueries: ConversationQueries {
	private var pending: CheckedContinuation<Void, Never>?
	private var writes: [String] = []
	func list(consent _: ConversationConsent, limit _: Int) async throws -> [ConversationRecord] {
		[]
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id: String, consent _: ConversationConsent) async throws {
		writes.append(id)
		if writes.count == 1 {
			await withCheckedContinuation { pending = $0 }
		}
	}

	func waiting() -> Bool {
		pending != nil
	}

	func resume() {
		pending?.resume(); pending = nil
	}

	func writtenIDs() -> [String] {
		writes
	}
}

@Test @MainActor func staleConsentCompletionReleasesCommandOwnership() async throws {
	let fence = SessionFence(), routes = RouteOwner(), queries = DelayedConsentQueries()
	let session = try #require(fence.replace(UUID()))
	let store = RequestsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: queries,
		preferences: WindowPreferences(),
	)
	store.send(.accept("first"))
	while await !(queries.waiting()) {
		await Task.yield()
	}
	routes.show(.settings)
	await queries.resume()
	await store.waitForOperation()
	#expect(store.state.busy)
	#expect(routes.current == .settings)
	store.send(.accept("second"))
	await store.waitForOperation()
	#expect(await queries.writtenIDs() == ["first", "second"])
	#expect(!store.state.busy)
	#expect(routes.current == .chat("second"))
}

private actor SupersededLoadQueries: ConversationQueries {
	enum Failure: Error { case oldRequest }
	private var requests = 0
	private var pending: [Int: CheckedContinuation<[ConversationRecord], Error>] = [:]
	func list(consent: ConversationConsent, limit _: Int) async throws -> [ConversationRecord] {
		guard consent == .allowed else { return [] }
		requests += 1
		let request = requests
		return try await withCheckedThrowingContinuation { pending[request] = $0 }
	}

	func started(_ request: Int) -> Bool {
		pending[request] != nil
	}

	func failOld() {
		pending.removeValue(forKey: 1)?.resume(throwing: Failure.oldRequest)
	}

	func finishNew() {
		pending.removeValue(forKey: 2)?.resume(returning: [ConversationRecord(id: "new", kind: .group)])
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		record
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
}

@Test @MainActor func supersededLoadFailureCannotChangeNewLoad() async throws {
	let fence = SessionFence(), routes = RouteOwner(), queries = SupersededLoadQueries()
	let session = try #require(fence.replace(UUID()))
	let store = ConversationsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: queries,
		preferences: WindowPreferences(),
		ownInboxID: "own",
	)
	let old = Task { await store.refresh() }
	while await !(queries.started(1)) {
		await Task.yield()
	}
	let new = Task { await store.refresh() }
	while await !(queries.started(2)) {
		await Task.yield()
	}
	await queries.failOld()
	await old.value
	#expect(store.state.loading)
	#expect(store.state.error == nil)
	await queries.finishNew()
	await new.value
	#expect(store.state.rows.map(\.id) == ["new"])
	#expect(!store.state.loading)
	#expect(store.state.error == nil)
}

private actor FilteredWindowQueries: ConversationQueries {
	private var reads: [String] = []
	private var lists = 0
	func list(consent: ConversationConsent, limit: Int) async throws -> [ConversationRecord] {
		guard consent == .allowed else { return [] }
		lists += 1
		return (0 ..< min(100, limit)).map { ConversationRecord(id: String($0), kind: .group) }
	}

	func details(_ record: ConversationRecord, marker _: Int64) async throws -> ConversationRecord {
		reads.append(record.id)
		var result = record
		result.name = "Named" + record.id
		return result
	}

	func setConsent(id _: String, consent _: ConversationConsent) async throws {}
	func clearReads() {
		reads = []
	}

	func detailReads() -> [String] {
		reads
	}

	func listReads() -> Int {
		lists
	}
}

@Test @MainActor func filteredVisibleIndexHydratesDisplayedConversation() async throws {
	let fence = SessionFence(), routes = RouteOwner(), queries = FilteredWindowQueries()
	let session = try #require(fence.replace(UUID()))
	let store = ConversationsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: queries,
		preferences: WindowPreferences(),
		ownInboxID: "own",
	)
	store.send(.load)
	await store.waitForLoad()
	store.send(.loadMore)
	await store.waitForLoad()
	await queries.clearReads()
	store.send(.search("70"))
	#expect(store.state.visibleRows.map(\.id) == ["70"])
	store.send(.visible(start: 0, count: 1))
	await store.waitForLoad()
	#expect(await queries.detailReads() == ["70"])
	#expect(store.state.visibleRows.first?.title == "Named70")
	#expect(await queries.listReads() == 2)
}

@Test func conversationAvatarInitials() {
	for (title, expected) in [("Protocol design", "PD"), ("Release crew", "RC"), ("Group", "G")] {
		let record = ConversationRecord(id: "group", kind: .group, name: title)
		let row = ConversationRow(conversation: record, title: title, muted: false)
		#expect(DisplayInitials.group(title) == expected)
		#expect(row.initials == expected)
	}
	for (title, expected) in [("Maya", "M"), ("@lena", "L")] {
		let record = ConversationRecord(id: "dm", kind: .dm)
		let row = ConversationRow(conversation: record, title: title, muted: false)
		#expect(DisplayInitials.person(title) == expected)
		#expect(row.initials == expected)
	}
	let inboxID = "0f1234567890abcdef"
	let record = ConversationRecord(id: "dm", kind: .dm, peerInboxID: inboxID)
	let row = ConversationRow(conversation: record, title: record.title(), muted: false)
	#expect(DisplayInitials.person(inboxID, isInboxID: true) == "0f")
	#expect(row.initials == "0f")
}

@Test @MainActor func acceptingRequestKeepsChatsAsNavigationRoot() async throws {
	let fence = SessionFence(), routes = RouteOwner(), queries = DelayedConsentQueries()
	let session = try #require(fence.replace(UUID()))
	routes.replace(.chats)
	routes.show(.requests)
	let store = RequestsStore(
		session: session,
		fence: fence,
		routes: routes,
		queries: queries,
		preferences: WindowPreferences(),
	)
	store.send(.accept("accepted"))
	while await !(queries.waiting()) {
		await Task.yield()
	}
	#expect(routes.current == .requests)
	await queries.resume()
	await store.waitForOperation()
	#expect(await queries.writtenIDs() == ["accepted"])
	#expect(routes.root == .chats)
	#expect(routes.path == [.chat("accepted")])
	#expect(routes.current == .chat("accepted"))
	routes.back()
	#expect(routes.current == .chats)
	#expect(routes.path.isEmpty)
}
