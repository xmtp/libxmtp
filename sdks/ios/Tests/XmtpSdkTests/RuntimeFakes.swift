import Foundation
@testable import XmtpSdk

// These fakes have no Rust handle. They override each generated call that a
// runtime path makes, so the tests run the real Swift runtime without a backend.

enum ReadFailure: Error {
	case failed
}

/// A Client fake. Each test sets the values that its runtime path reads.
final class FakeClient: Client, @unchecked Sendable {
	var eventReader: EventReader?
	var fakeConversations: Conversations?
	var listenerStarted: (@Sendable (EventListener) async -> ListenerId)?

	override func clientKey() -> UInt64 {
		UInt64(UInt(bitPattern: ObjectIdentifier(self).hashValue))
	}

	override func end() async throws {}

	override func events(filter _: EventFilter) async throws -> EventReader {
		guard let eventReader else { fatalError("The test did not set an event reader") }
		return eventReader
	}

	override func conversations() -> Conversations {
		guard let fakeConversations else { fatalError("The test did not set conversations") }
		return fakeConversations
	}

	override func startListener(filter _: EventFilter, listener: EventListener) async throws -> ListenerId {
		guard let listenerStarted else { fatalError("The test did not set a listener start") }
		return await listenerStarted(listener)
	}

	override func stopListener(id _: ListenerId) async {}
}

func makeSDKClient(_ raw: FakeClient) -> SDKClient {
	SDKClient(raw, codecs: [])
}

/// Read 1 waits until the test releases it or ends the reader. Read 2 returns 2.
/// Each later read returns nil.
actor HeldReads {
	private var calls = 0
	private var ends = 0
	private var ended = false
	private var entered = false
	private var enteredWaiter: CheckedContinuation<Void, Never>?
	private var release: CheckedContinuation<Void, Never>?
	private let failFirst: Bool

	init(failFirst: Bool = false) {
		self.failFirst = failFirst
	}

	func next() async throws -> Int? {
		calls += 1
		if calls == 1 {
			entered = true
			enteredWaiter?.resume()
			enteredWaiter = nil
			await withCheckedContinuation { release = $0 }
			if ended {
				return nil
			}
			try Task.checkCancellation()
			if failFirst {
				throw ReadFailure.failed
			}
			return 1
		}
		return calls == 2 ? 2 : nil
	}

	func waitForRead() async {
		if entered {
			return
		}
		await withCheckedContinuation { enteredWaiter = $0 }
	}

	func releaseRead() {
		release?.resume()
		release = nil
	}

	func end() {
		ended = true
		ends += 1
		release?.resume()
		release = nil
	}

	func counts() -> (reads: Int, ends: Int) {
		(calls, ends)
	}
}

/// Event read N returns a marker event whose group ID is the byte N.
final class FakeEventReader: EventReader, @unchecked Sendable {
	private let reads: HeldReads

	init(_ reads: HeldReads) {
		self.reads = reads
		super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("A fake has no Rust handle")
	}

	static func marker(_ index: Int) -> ClientEvent {
		.conversationForkDetected(conversationForkDetected: GroupRef(groupId: Data([UInt8(index)])))
	}

	override func next() async throws -> ClientEvent? {
		try await reads.next().map(Self.marker)
	}

	override func end() async throws {
		await reads.end()
	}
}

/// Conversation read N returns `.group(groups[N - 1])`.
final class FakeConversationReader: ConversationReader, @unchecked Sendable {
	let groups = [Group(noHandle: Group.NoHandle()), Group(noHandle: Group.NoHandle())]
	private let reads: HeldReads

	init(_ reads: HeldReads) {
		self.reads = reads
		super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("A fake has no Rust handle")
	}

	/// The 1-based read index of a returned conversation, or nil.
	func index(of conversation: Conversation?) -> Int? {
		guard case let .group(group)? = conversation else {
			return nil
		}
		return groups.firstIndex { $0 === group }.map { $0 + 1 }
	}

	override func next() async throws -> Conversation? {
		try await reads.next().map { .group(group: groups[$0 - 1]) }
	}

	override func end() async throws {
		await reads.end()
	}

	override func connectionState() async -> ConnectionState {
		.connected
	}

	override func connectionStateChanged(previous _: ConnectionState) async throws -> ConnectionState {
		.closed
	}
}

final class FakeConversations: Conversations, @unchecked Sendable {
	private let reader: ConversationReader

	init(_ reader: ConversationReader) {
		self.reader = reader
		super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("A fake has no Rust handle")
	}

	override func conversationReader(options _: ConversationReaderOptions?) async throws -> ConversationReader {
		reader
	}
}
