import XCTest
@testable import XmtpSdk

private final class CloseCount: @unchecked Sendable {
	private let lock = NSLock()
	private var count = 0

	func increment() {
		lock.lock()
		count += 1
		lock.unlock()
	}

	func value() -> Int {
		lock.lock()
		defer { lock.unlock() }
		return count
	}
}

/// One reader iterator allows one read at a time. A rejected read does not read,
/// end or close the active reader. A failed or cancelled read closes the iterator.
final class ReaderIteratorReadTests: XCTestCase {
	private func makeIterator(
		_ reads: HeldReads,
		closed: CloseCount? = nil,
	) async throws -> (SDKConversationStream.Iterator, FakeConversationReader, SDKClient) {
		let reader = FakeConversationReader(reads)
		let raw = FakeClient(noHandle: Client.NoHandle())
		raw.fakeConversations = FakeConversations(reader)
		let client = makeSDKClient(raw)
		let onClose: (@Sendable (SDKStreamCloseReason) throws -> Void)? = closed.map { count in
			{ @Sendable _ in count.increment() }
		}
		let stream = try await client.conversations.stream(options: .init(onClose: onClose))
		return (stream.makeAsyncIterator(), reader, client)
	}

	func testOverlappingReadIsRejectedWithoutReadEndOrClose() async throws {
		let reads = HeldReads()
		let closed = CloseCount()
		let (iterator, reader, client) = try await makeIterator(reads, closed: closed)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()

		var rejected: ErrorDetails?
		do {
			_ = try await iterator.next()
			XCTFail("An overlapping read returned a value")
		} catch let XmtpError.ConsumerOwned(details) {
			rejected = details
		}
		let beforeRelease = await reads.counts()
		let closesBeforeRelease = closed.value()
		await reads.releaseRead()
		let firstValue = try await first.value

		XCTAssertEqual(rejected?.code, "ConsumerOwned")
		XCTAssertEqual(rejected?.retryable, false)
		XCTAssertEqual(beforeRelease.reads, 1, "The rejected read advanced the reader")
		XCTAssertEqual(beforeRelease.ends, 0, "The rejected read ended the reader")
		XCTAssertEqual(closesBeforeRelease, 0, "The rejected read closed the stream")
		XCTAssertEqual(reader.index(of: firstValue), 1)
		let secondValue = try await iterator.next()
		XCTAssertEqual(reader.index(of: secondValue), 2, "A successful read kept read ownership")
		let last = try await iterator.next()
		XCTAssertNil(last, "The stream did not end")
		let afterEnd = await reads.counts()
		XCTAssertEqual(afterEnd.ends, 1, "The reader did not end exactly once")
		XCTAssertEqual(closed.value(), 1, "The close callback did not run exactly once")
		withExtendedLifetime(client) {}
	}

	func testFailedReadClosesTheIterator() async throws {
		let reads = HeldReads(failFirst: true)
		let (iterator, _, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()
		await reads.releaseRead()
		do {
			_ = try await first.value
			XCTFail("The read failure did not throw")
		} catch ReadFailure.failed {}

		do {
			_ = try await iterator.next()
			XCTFail("The failed iterator did not close")
		} catch is CancellationError {}
		withExtendedLifetime(client) {}
	}

	func testCancelledReadClosesTheIterator() async throws {
		let reads = HeldReads()
		let (iterator, _, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()
		first.cancel()
		let settled = expectation(description: "The cancelled read settles")
		let watcher = Task {
			_ = try? await first.value
			settled.fulfill()
		}
		if await XCTWaiter.fulfillment(of: [settled], timeout: 2) != .completed {
			XCTFail("Cancellation did not end the active reader")
			await reads.releaseRead()
		}
		await watcher.value

		do {
			_ = try await iterator.next()
			XCTFail("The cancelled iterator did not close")
		} catch is CancellationError {}
		withExtendedLifetime(client) {}
	}
}
