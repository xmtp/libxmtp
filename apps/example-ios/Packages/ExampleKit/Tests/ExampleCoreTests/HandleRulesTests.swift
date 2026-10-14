@testable import ExampleCore
import Foundation
import Testing

@Test func handleGrammarOrder() {
	let cases: [(String, HandleProblem?)] = [
		("a!#x", .invalidCharacters),
		("1#x", .startWithLetter),
		(String(repeating: "a", count: 28) + "#x", .tooLong),
		("a#x", .tooShort),
		("abc#x", .badCode),
		("abc#1234", nil),
	]
	for (input, problem) in cases {
		#expect(HandleRules.problem(input, minimum: 3) == problem)
	}
	#expect(HandleRules.normalize("AbÇ") == "abÇ")
}

@Test func recipientParsing() throws {
	#expect(throws: IdentityError.empty) { try IdentityInput.parse(" ") }
	#expect(throws: IdentityError.invalidAddress) { try IdentityInput.parse("0xabc") }
	#expect(throws: IdentityError.invalidInbox) { try IdentityInput.parse(String(repeating: "z", count: 64)) }
	#expect(try IdentityInput.parse(" @A#1234 ") == .handle("a#1234"))
	#expect(try IdentityInput.parse("0X" + String(repeating: "A", count: 40)) == .ethereum("0x" + String(
		repeating: "a",
		count: 40,
	)))
	#expect(try IdentityInput.parse(String(repeating: "B", count: 64)) == .inbox(String(repeating: "b", count: 64)))
	#expect(HandleRules.problem("a", minimum: 3) == .tooShort)
	#expect(try IdentityInput.parse("a") == .handle("a"))
}

@MainActor private final class HandleFake: HandleAccount {
	var claimHook: (() async -> Void)?
	var minimum = 3; var claimTimes: [String: Date] = [:]; var claims = 0; var recordFailures = 0; var records: [Date] =
		[]; var error: HandleError?; var answers = [
			"abc#0001",
			"abc#0002",
			"abc#0003",
			"abc#0004",
		]
	func handles() async throws -> OwnHandles {
		OwnHandles(current: nil)
	}

	func availableHandles(name _: String) async throws -> [String] {
		answers
	}

	func claimHandle(handle _: String) async throws {
		claims += 1; await claimHook?(); if let error {
			throw error
		}
	}

	func recordClaim(handle: String, at date: Date) throws {
		records
			.append(date); if recordFailures > 0
		{
			recordFailures -= 1; throw HandleError.failed
		}; claimTimes[handle] = date
	}
}

@MainActor @Test func suggestionsAndClaimFailures() async {
	let fake = HandleFake(); let store = HandleStore(
		account: fake,
		mode: .choose,
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: {},
	)
	store.send(.load); for _ in 0 ..< 20 {
		await Task.yield()
	}
	store.send(.changeInput("abc")); store.send(.check); for _ in 0 ..< 20 {
		await Task.yield()
	}
	#expect(store.state.status.visible.count == 3); #expect(fake.claims == 0)
	store.send(.showMore); #expect(store.state.status.visible.count == 4)
	fake.error = .taken; store.send(.claim); for _ in 0 ..< 20 {
		await Task.yield()
	}
	#expect(!store.state.status.visible.contains("abc#0001")); #expect(store.state.status.pick == "abc#0002"); #expect(fake
		.claims == 1)
	fake.error = .tooSoon; store.send(.claim); for _ in 0 ..< 20 {
		await Task.yield()
	}; #expect(store.state.blocked)
}

private struct IdentityFake: IdentityQueries {
	let result: String?; let fail: Bool
	func inboxID(for _: IdentityInput) async throws -> String? {
		if fail {
			throw HandleError.failed
		}; return result
	}

	func handles(inboxIDs _: [String]) async throws -> [InboxIdentity] {
		[]
	}
}

@Test func recipientLookupFailures() async throws {
	let handle = try IdentityInput.parse("a")
	await #expect(throws: IdentityError.handleNotFound) { try await handle.resolve(using: IdentityFake(
		result: nil,
		fail: false,
	)) }
	await #expect(throws: HandleError.failed) { try await handle.resolve(using: IdentityFake(result: nil, fail: true)) }
	#expect(try await handle.resolve(using: IdentityFake(result: "resolved", fail: false)) == "resolved")
}

@MainActor @Test func claimLimitLocksEditor() async {
	let fake = HandleFake(); fake.error = .limitReached
	let store = HandleStore(
		account: fake,
		mode: .choose,
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: {},
	)
	store.send(.load); for _ in 0 ..< 20 {
		await Task.yield()
	}; store.send(.changeInput("abc")); store
		.send(.check); for _ in 0 ..< 20
	{
		await Task.yield()
	}; store.send(.claim); for _ in 0 ..< 20 {
		await Task.yield()
	}
	#expect(store.state.notice == .limitReached); #expect(!store.state.editable)
	store.send(.changeInput("different")); #expect(store.state.input.isEmpty)
}

@MainActor @Test func handleChangePreservesOtherHandles() {
	let directory = IdentityDirectory(queries: IdentityFake(result: nil, fail: false), valid: { true })
	directory.merge([InboxIdentity(inboxID: "inbox", handle: "first", otherHandles: ["older", "first", "next"])])
	directory.handleChanged(inboxID: "inbox", handle: "next", otherHandles: [])
	#expect(directory.identities["inbox"]?.otherHandles == ["first", "older"])
	#expect(directory.displayName(for: "inbox") == "@next")
	directory.handleChanged(inboxID: "inbox", handle: "latest", otherHandles: ["authoritative", "authoritative", "latest"])
	#expect(directory.identities["inbox"]?.otherHandles == ["authoritative"])
}

@MainActor private final class DelayedHandleAccount: HandleAccount {
	var minimum = 3
	var claimTimes: [String: Date] = [:]
	private var response: CheckedContinuation<OwnHandles, Never>?
	private var started: CheckedContinuation<Void, Never>?
	func handles() async throws -> OwnHandles {
		await withCheckedContinuation { continuation in
			response = continuation; started?.resume(); started = nil
		}
	}

	func waitUntilLoading() async {
		if response != nil {
			return
		}
		await withCheckedContinuation { started = $0 }
	}

	func completeLoad() {
		response?.resume(returning: OwnHandles(current: nil)); response = nil
	}

	func availableHandles(name: String) async throws -> [String] {
		[name]
	}

	func claimHandle(handle _: String) async throws {}
	func recordClaim(handle _: String, at _: Date) throws {}
}

@MainActor @Test func editingDuringInitialHandleLoadRemainsUsable() async {
	let account = DelayedHandleAccount()
	let store = HandleStore(
		account: account,
		mode: .choose,
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: {},
	)
	store.send(.load)
	await account.waitUntilLoading()
	store.send(.changeInput("alice"))
	#expect(store.state.loading)
	account.completeLoad()
	for _ in 0 ..< 1000 {
		if !store.state.loading {
			break
		}; await Task.yield()
	}
	#expect(!store.state.loading)
	#expect(store.state.input == "alice")
	#expect(store.state.canCheck)
	await store.close()
}

private actor DelayedIdentityQueries: IdentityQueries {
	private var response: CheckedContinuation<[InboxIdentity], Never>?
	private var started: CheckedContinuation<Void, Never>?
	func inboxID(for _: IdentityInput) async throws -> String? {
		nil
	}

	func handles(inboxIDs _: [String]) async throws -> [InboxIdentity] {
		await withCheckedContinuation { continuation in response = continuation; started?.resume(); started = nil }
	}

	func waitUntilLoading() async {
		if response == nil {
			await withCheckedContinuation { started = $0 }
		}
	}

	func complete(_ values: [InboxIdentity]) {
		response?.resume(returning: values); response = nil
	}
}

@MainActor @Test func handleEventWinsOverEarlierInboxRead() async throws {
	let queries = DelayedIdentityQueries()
	let directory = IdentityDirectory(queries: queries, valid: { true })
	let read = Task { try await directory.load(["inbox", "other"]) }
	await queries.waitUntilLoading()
	directory.handleChanged(inboxID: "inbox", handle: "new", otherHandles: [])
	await queries.complete([
		InboxIdentity(inboxID: "inbox", handle: "old"),
		InboxIdentity(inboxID: "other", handle: "unaffected"),
	])
	try await read.value
	#expect(directory.displayName(for: "inbox") == "@new")
	#expect(directory.displayName(for: "other") == "@unaffected")
}

@MainActor @Test func failedClaimRecordRetriesOnlyLocalSave() async {
	let account = HandleFake(); account.recordFailures = 1
	var finished = 0
	let store = HandleStore(
		account: account,
		mode: .choose,
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: { finished += 1 },
	)
	store.send(.load); for _ in 0 ..< 1000 {
		if !store.state.loading {
			break
		}; await Task.yield()
	}
	store.send(.changeInput("abc")); store.send(.check)
	for _ in 0 ..< 1000 {
		if store.state.status.pick != nil {
			break
		}; await Task.yield()
	}
	store.send(.claim)
	for _ in 0 ..< 1000 {
		if store.state.claiming == nil {
			break
		}; await Task.yield()
	}
	#expect(store.state.current == "abc#0001")
	#expect(store.state.notice == .recordFailed("abc#0001"))
	#expect(!store.state.editable)
	#expect(finished == 0)
	store.send(.skip); #expect(finished == 0)
	store.send(.retry)
	#expect(account.claims == 1)
	#expect(account.records.count == 2)
	#expect(account.records.first == account.records.last)
	#expect(account.claimTimes["abc#0001"] == account.records.first)
	#expect(finished == 1)
	await store.close()
}

@MainActor @Test func successfulClaimPersistsAfterRouteCloses() async throws {
	let account = HandleFake(); let gate = ReviewGate()
	account.claimHook = { await gate.wait() }
	let store = HandleStore(
		account: account,
		mode: .choose,
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in true },
		finish: {},
		sessionValid: { _ in true },
	)
	store.send(.changeInput("abc")); store.send(.check)
	for _ in 0 ..< 10000 {
		if store.state.status.pick != nil {
			break
		}; await Task.yield()
	}
	store.send(.claim)
	for _ in 0 ..< 10000 {
		if gate.started {
			break
		}; await Task.yield()
	}
	try #require(gate.started)
	let closing = Task { await store.close() }; await Task.yield(); gate.release(); await closing.value
	#expect(account.claimTimes["abc#0001"] != nil)
}
