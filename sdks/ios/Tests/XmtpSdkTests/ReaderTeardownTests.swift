import Foundation
import XCTest
@testable import XmtpSdk

/// A message reader with no Rust object. Each test sets the calls it needs.
private final class ScriptedReader: MessageReader, @unchecked Sendable {
	private let read: @Sendable () async throws -> Message?
	private let finish: @Sendable () async -> Void
	private let state: @Sendable () async -> ConnectionState
	private let stateChanged: @Sendable (ConnectionState) async throws -> ConnectionState

	init(
		read: @escaping @Sendable () async throws -> Message?,
		finish: @escaping @Sendable () async -> Void = {},
		state: @escaping @Sendable () async -> ConnectionState = { .connected },
		stateChanged: @escaping @Sendable (ConnectionState) async throws -> ConnectionState = { _ in
			try await pause(seconds: 10)
			return .closed
		},
	) {
		self.read = read
		self.finish = finish
		self.state = state
		self.stateChanged = stateChanged
		super.init(noHandle: MessageReader.NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("A fake has no Rust handle")
	}

	override func next() async throws -> Message? {
		try await read()
	}

	override func end() async throws {
		await finish()
	}

	override func connectionState() async -> ConnectionState {
		await state()
	}

	override func connectionStateChanged(previous: ConnectionState) async throws -> ConnectionState {
		try await stateChanged(previous)
	}
}

/// Holds a late reader open until the test releases it. It ignores cancellation.
private actor LateOpenGate {
	private var released = false
	private var waiting: CheckedContinuation<Void, Never>?

	func hold() async {
		if released {
			return
		}
		await withCheckedContinuation { waiting = $0 }
	}

	func release() {
		released = true
		waiting?.resume()
		waiting = nil
	}
}

private struct AppStop: Error {}

/// The Swift reader stream (`runtime/streams/Readers.swift`) releases its reader
/// before iteration ends, ends a reader that opens late, and stops its state
/// monitor at Closed. The tests open streams through `makeSDKMessageStream`,
/// the function that every `SDKClient.messages` form uses, so a test can hold
/// the reader that the stream opens.
final class ReaderTeardownTests: XCTestCase {
	private func fakeOwner() -> SDKClient {
		makeSDKClient(FakeClient(noHandle: Client.NoHandle()))
	}

	/// The stored sequence stays alive after the app throws. Its loop iterator
	/// must release the reader. The app error must not acknowledge the item.
	// verifies: PROC-052, PROC-031, PROC-041
	func testAppErrorEndsTheReaderWithoutAcknowledgingTheItem() async throws {
		try await withLiveClients(1) { clients in
			let owner = clients[0]
			let group = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
			let heldId = try await group.sendText(text: "held by the app", options: nil)
			let opened = Shared<MessageReader?>(nil)
			let reasons = Shared<[SDKStreamCloseReason]>([])
			let stream = makeSDKMessageStream(open: {
				// Keep the raw reader so that native destruction cannot hide a
				// missing end call.
				let reader = try await group.messageReader(options: nil)
				opened.update { $0 = reader }
				return reader
			}, owner: owner, onClose: { reason in
				reasons.update { $0.append(reason) }
			}, onConnectionStateChange: nil)
			var delivered: MessageId?
			do {
				for try await message in stream {
					delivered = message.id
					throw AppStop()
				}
				XCTFail("The loop ended without an item")
			} catch is AppStop {}
			XCTAssertEqual(delivered, heldId, "The loop received the wrong message")

			// Release starts an asynchronous teardown. Wait for its close signal;
			// the end of the loop does not prove that teardown has finished.
			let closed = await eventually(seconds: 10) { !reasons.value.isEmpty }
			XCTAssertTrue(closed, "The loop did not close its reader")
			try await pause(seconds: 0.1)
			XCTAssertEqual(reasons.value.count, 1, "The loop notified close more than once")
			if case .failed? = reasons.value.first {
				XCTFail("An app error closed the stream as failed")
			}
			guard let rawReader = opened.value else {
				return XCTFail("The stream did not open a reader")
			}
			let rawState = await rawReader.connectionState()
			XCTAssertEqual(rawState, .closed, "The loop did not end the raw reader")

			let replay = try await group.messageReader(options: nil)
			let repeated = try await within(seconds: 10) { try await replay.next() }
			try await replay.end()
			XCTAssertEqual(repeated??.id, heldId, "The app error acknowledged the held item")
			withExtendedLifetime(stream) {}
		}
	}

	/// Cancellation reaches an open that waits cooperatively, before the test
	/// releases it.
	// verifies: PROC-041
	func testCancellationReachesACooperativeOpen() async throws {
		let owner = fakeOwner()
		let entered = Shared(false)
		let cancelled = Shared(false)
		let returned = Shared(false)
		let closes = Shared(0)
		let (release, releaseSignal) = AsyncStream<Void>.makeStream()
		defer { releaseSignal.finish() }
		let stream = makeSDKMessageStream(open: {
			entered.update { $0 = true }
			var iterator = release.makeAsyncIterator()
			// An AsyncStream read returns nil when its task is cancelled.
			_ = await iterator.next()
			if Task.isCancelled {
				cancelled.update { $0 = true }
			}
			throw CancellationError()
		}, owner: owner, onClose: { reason in
			if case .closed = reason {
				closes.update { $0 += 1 }
			}
		}, onConnectionStateChange: nil)
		let operation = Task {
			defer { returned.update { $0 = true } }
			let iterator = stream.makeAsyncIterator()
			return try await iterator.next()
		}
		defer { operation.cancel() }
		let started = await eventually(seconds: 2) { entered.value }
		guard started else {
			operation.cancel()
			releaseSignal.finish()
			_ = try? await operation.value
			return XCTFail("The reader open did not start")
		}
		operation.cancel()
		let completedWithoutRelease = await eventually(seconds: 2) { returned.value }
		// Release only after the deadline.
		releaseSignal.finish()
		do {
			_ = try await operation.value
			XCTFail("The cancelled open delivered a message")
		} catch is CancellationError {}
		XCTAssertTrue(completedWithoutRelease, "The cancelled read waited for the manual release")
		XCTAssertTrue(cancelled.value, "The open did not receive cancellation")
		XCTAssertEqual(closes.value, 1, "The stream did not close exactly once")
		withExtendedLifetime((owner, stream)) {}
	}

	/// A late open can ignore cancellation. Close waits until its reader ends.
	// verifies: PROC-041
	func testCloseWaitsForALateOpenToEndItsReader() async throws {
		try await withLiveClients(1) { clients in
			let owner = clients[0]
			let group = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
			let opened = Shared<MessageReader?>(nil)
			let gate = LateOpenGate()
			let closes = Shared(0)
			let opening = Task {
				let stream = makeSDKMessageStream(open: {
					let reader = try await group.messageReader(options: nil)
					opened.update { $0 = reader }
					await gate.hold()
					return reader
				}, owner: owner, onClose: { reason in
					if case .closed = reason {
						closes.update { $0 += 1 }
					}
				}, onConnectionStateChange: nil)
				return try await stream.makeAsyncIterator().next()
			}
			defer { opening.cancel() }
			let didOpen = await eventually(seconds: 10) { opened.value != nil }
			guard didOpen, let lateReader = opened.value else {
				opening.cancel()
				await gate.release()
				_ = try? await opening.value
				return XCTFail("The reader did not open before cancellation")
			}
			opening.cancel()
			try await pause(seconds: 0.1)
			let closesBeforeRelease = closes.value
			await gate.release()
			do {
				_ = try await opening.value
				XCTFail("The cancelled open delivered a message")
			} catch is CancellationError {}
			XCTAssertEqual(closesBeforeRelease, 0, "Close ran before the late reader ended")
			let readerClosed = await eventually(seconds: 10) { await lateReader.connectionState() == .closed }
			XCTAssertTrue(readerClosed, "The late reader was not closed")
			let notified = await eventually(seconds: 1) { closes.value == 1 }
			XCTAssertTrue(notified, "The late reader did not notify close")
			let next = try await within(seconds: 10) { try await lateReader.next() }
			XCTAssertNotNil(next, "A read on the late reader did not return")
			XCTAssertNil(next ?? nil, "The late reader still delivered a message")
		}
	}

	/// When iteration ends, the reader is already released: a replacement reader
	/// on the same group opens at once. A slow end makes a detached teardown lose
	/// this race every time.
	func testIterationEndsAfterTheReaderIsReleased() async throws {
		try await withLiveClients(1) { clients in
			let owner = clients[0]
			let scope = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
			func slowEndStream(read: @escaping @Sendable () async throws -> Message?) -> SDKMessageStream {
				makeSDKMessageStream(open: {
					let reader = try await scope.messageReader(options: nil)
					return ScriptedReader(read: read, finish: {
						try? await pause(seconds: 0.2)
						try? await reader.end()
					})
				}, owner: owner, onClose: nil, onConnectionStateChange: nil)
			}
			func reopenAfter(_ path: String) async throws {
				do {
					let replacement = try await scope.messageReader(options: nil)
					try await replacement.end()
				} catch XmtpError.ConsumerOwned {
					XCTFail("\(path) ended iteration before the reader was released")
				}
			}

			let ended = try await slowEndStream(read: { nil }).makeAsyncIterator().next()
			XCTAssertNil(ended, "An ended stream delivered a message")
			try await reopenAfter("End of stream")

			do {
				_ = try await slowEndStream(read: { throw ReadFailure.failed }).makeAsyncIterator().next()
				XCTFail("A failed read delivered a message")
			} catch ReadFailure.failed {}
			try await reopenAfter("A read failure")

			let cancelledRead = Task {
				try await slowEndStream(read: {
					try await pause(seconds: 10)
					return nil
				}).makeAsyncIterator().next()
			}
			try await pause(seconds: 0.1)
			cancelledRead.cancel()
			_ = try? await cancelledRead.value
			try await reopenAfter("Cancellation")
		}
	}

	/// A conversation stream delivers a group that is created after its reader
	/// opens. The first connection state is reported only after the open.
	func testConversationStreamDeliversAGroupCreatedAfterItsReaderOpens() async throws {
		try await withLiveClients(1) { clients in
			let owner = clients[0]
			let opened = Shared(false)
			let stream = try await owner.conversationStream(onConnectionStateChange: { _, _ in
				opened.update { $0 = true }
			})
			let iterator = stream.makeAsyncIterator()
			let pending = Task { try await iterator.next() }
			defer { pending.cancel() }
			let isOpen = await eventually(seconds: 10) { opened.value }
			guard isOpen else {
				return XCTFail("The conversation reader did not open")
			}
			let group = try await owner.conversations().createGroup(members: [InboxId](), options: nil)
			let delivered = try await within(seconds: 10) { try await pending.value }
			guard case let .group(received)?? = delivered else {
				return XCTFail("The conversation stream did not deliver the group before the deadline")
			}
			XCTAssertEqual(received.id(), group.id(), "The conversation stream delivered another conversation")
		}
	}

	/// The state monitor stops reading after Closed. A reader that opens on a
	/// connected connection reports Connected first.
	// verifies: PROC-044
	func testStateMonitorStopsAtClosedAndReportsTheOpeningState() async throws {
		let owner = fakeOwner()
		let monitorCalls = Shared(0)
		let reportedClosed = Shared(false)
		let closing = makeSDKMessageStream(open: {
			ScriptedReader(read: {
				try await pause(seconds: 10)
				return nil
			}, state: { .connecting }, stateChanged: { _ in
				monitorCalls.update { $0 += 1 }
				return .closed
			})
		}, owner: owner, onClose: nil, onConnectionStateChange: { _, current in
			if current == .closed {
				reportedClosed.update { $0 = true }
			}
		})
		let closingRead = Task { try await closing.makeAsyncIterator().next() }
		let sawClosed = await eventually(seconds: 5) { reportedClosed.value }
		let callsAtClosed = monitorCalls.value
		try await pause(seconds: 0.1)
		closingRead.cancel()
		_ = try? await closingRead.value
		XCTAssertTrue(sawClosed, "The state monitor did not report Closed")
		XCTAssertEqual(callsAtClosed, 1, "The state monitor did not wait for one change")
		XCTAssertEqual(monitorCalls.value, callsAtClosed, "The state monitor kept reading after Closed")

		let states = Shared<[ConnectionState]>([])
		let connected = makeSDKMessageStream(open: {
			ScriptedReader(read: {
				try await pause(seconds: 10)
				return nil
			}, state: { .connected })
		}, owner: owner, onClose: nil, onConnectionStateChange: { _, current in
			states.update { $0.append(current) }
		})
		let connectedRead = Task { try await connected.makeAsyncIterator().next() }
		let reported = await eventually(seconds: 5) { !states.value.isEmpty }
		connectedRead.cancel()
		_ = try? await connectedRead.value
		XCTAssertTrue(reported, "The connected reader reported no state")
		XCTAssertEqual(states.value.first, .connected, "The connected reader reported another state first")
		withExtendedLifetime(owner) {}
	}
}
