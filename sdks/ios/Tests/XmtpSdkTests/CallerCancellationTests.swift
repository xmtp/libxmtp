import Foundation
import XCTest
@testable import XmtpSdk

private func cancellationOptions() -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: StorageOptions(location: .inMemory),
		deviceSync: false,
	)
}

private actor StartGate {
	private var open = false
	private var waiting: [CheckedContinuation<Void, Never>] = []

	func wait() async {
		if open {
			return
		}
		await withCheckedContinuation { waiting.append($0) }
	}

	func release() {
		open = true
		for waiter in waiting {
			waiter.resume()
		}
		waiting.removeAll()
	}
}

/// These tests run the generated `uniffiRustCallAsync` glue against real native
/// futures. The Rust tests in `reader_ack_cancellation.rs` prove the Rust side.
/// These tests prove that Swift task cancellation reaches the native future and
/// that a cancelled read does not acknowledge or hand off an item. They cannot
/// stop a read at READY: that needs a gate inside the generated glue.
final class CallerCancellationTests: XCTestCase {
	/// Waits until one more native poll waits for its wake than at `baseline`.
	/// A read in that state is past its pre-poll cancellation check, so only
	/// the cancellation of its native future can end it.
	private func waitForPendingNativePoll(above baseline: Int) async -> Bool {
		let deadline = Date().addingTimeInterval(10)
		while UniffiNativePolls.shared.count <= baseline {
			if Date() > deadline {
				XCTFail("The read did not reach a pending native poll")
				return false
			}
			try? await Task.sleep(nanoseconds: 1_000_000)
		}
		return true
	}

	private func settle<Value: Sendable>(
		_ call: Task<Value, Error>,
		_ description: String,
	) async -> Result<Value, Error>? {
		let settled = expectation(description: description)
		let watcher = Task {
			let result = await call.result
			settled.fulfill()
			return result
		}
		guard await XCTWaiter.fulfillment(of: [settled], timeout: 10) == .completed else {
			XCTFail("\(description) did not settle")
			return nil
		}
		return await watcher.value
	}

	private func assertCancelled(_ result: Result<some Sendable, Error>?) {
		guard case let .failure(error)? = result else {
			return XCTFail("The cancelled read returned a value")
		}
		XCTAssertTrue(error is CancellationError, "Unexpected error \(error)")
	}

	/// A read cancelled before its first poll throws and does not acknowledge
	/// the prior item.
	func testReadCancelledBeforeFirstPollKeepsPriorItem() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: cancellationOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let first = try await group.sendText(text: "delivered before the cancelled read")
		let reader = try await group.messageReader(options: nil)
		let delivered = try await reader.next()
		XCTAssertEqual(delivered?.id, first)

		let gate = StartGate()
		let call = Task {
			await gate.wait()
			return try await reader.next()
		}
		call.cancel()
		await gate.release()
		let result = await settle(call, "The cancelled read before poll")
		guard result != nil else {
			try await client.end()
			return
		}
		assertCancelled(result)

		try await reader.end()
		let replay = try await group.messageReader(options: nil)
		let replayed = try await replay.next()
		XCTAssertEqual(replayed?.id, first, "The cancelled read acknowledged the prior item")
		try await replay.end()
		try await client.end()
	}

	/// Cancelling a pending read cancels the native future. Nothing else wakes
	/// it, because no message arrives. The reader stays open, and its next read
	/// returns the message sent after the cancellation.
	func testCancelledPendingReadEndsTheNativeFutureAndKeepsLaterItem() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: cancellationOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let reader = try await group.messageReader(options: nil)

		let baseline = UniffiNativePolls.shared.count
		let call = Task { try await reader.next() }
		guard await waitForPendingNativePoll(above: baseline) else {
			call.cancel()
			try await client.end()
			return
		}
		call.cancel()
		let result = await settle(call, "The cancelled pending read")
		guard result != nil else {
			try await client.end()
			return
		}
		assertCancelled(result)

		let later = try await group.sendText(text: "sent after the cancelled read")
		let next = try await reader.next()
		XCTAssertEqual(next?.id, later, "The cancelled read took a later item")
		try await reader.end()
		try await client.end()
	}

	/// A cancelled event read ends the reader and returns nil, so no event is
	/// handed off after cancellation.
	func testCancelledPendingEventReadEndsTheReader() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: cancellationOptions())
		let reader = try await client.raw.events(filter: EventFilter(kinds: [.conversationForkDetected]))
		let baseline = UniffiNativePolls.shared.count
		let call = Task { try await reader.next() }
		guard await waitForPendingNativePoll(above: baseline) else {
			call.cancel()
			try await client.end()
			return
		}
		call.cancel()
		guard let result = await settle(call, "The cancelled event read") else {
			try await client.end()
			return
		}
		switch result {
		case let .success(event):
			XCTAssertNil(event, "An event was handed off after cancellation")
		case let .failure(error):
			XCTFail("A cancelled event read threw \(error)")
		}
		let reopened = try await reader.next()
		XCTAssertNil(reopened, "The ended event reader read again")
		try await client.end()
	}
}
