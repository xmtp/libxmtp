import XCTest
@testable import XmtpSdk

/// One event iterator allows one read at a time. A rejected read does not read
/// or end the reader. Each read outcome releases read ownership.
// verifies: EVENT-015
final class EventIteratorReadTests: XCTestCase {
	private let filter = EventFilter(kinds: [.conversationForkDetected])

	private func makeIterator(_ reads: HeldReads) async throws -> (SDKEventStream.Iterator, SDKClient) {
		let raw = FakeClient(noHandle: Client.NoHandle())
		raw.eventReader = FakeEventReader(reads)
		let client = makeSDKClient(raw)
		return try await (client.events(filter).makeAsyncIterator(), client)
	}

	func testOverlappingReadIsRejectedWithoutReadOrEnd() async throws {
		let reads = HeldReads()
		let (iterator, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()

		var rejected: ErrorDetails?
		do {
			let value = try await iterator.next()
			XCTFail("An overlapping read returned \(String(describing: value))")
		} catch let XmtpError.ConsumerOwned(details) {
			rejected = details
		}
		let beforeRelease = await reads.counts()
		await reads.releaseRead()
		let firstValue = try await first.value

		XCTAssertEqual(rejected?.code, "ConsumerOwned")
		XCTAssertEqual(rejected?.retryable, false)
		XCTAssertEqual(beforeRelease.reads, 1, "The rejected read advanced the reader")
		XCTAssertEqual(beforeRelease.ends, 0, "The rejected read ended the reader")
		XCTAssertEqual(firstValue, FakeEventReader.marker(1))
		let secondValue = try await iterator.next()
		XCTAssertEqual(secondValue, FakeEventReader.marker(2), "A successful read kept read ownership")
		let last = try await iterator.next()
		XCTAssertNil(last)
		let afterEnd = await reads.counts()
		XCTAssertEqual(afterEnd.ends, 1, "The event reader did not end exactly once")
		withExtendedLifetime(client) {}
	}

	func testFailedReadReleasesReadOwnership() async throws {
		let reads = HeldReads(failFirst: true)
		let (iterator, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()
		await reads.releaseRead()
		do {
			_ = try await first.value
			XCTFail("The first read did not fail")
		} catch ReadFailure.failed {}

		let next = try await iterator.next()
		XCTAssertEqual(next, FakeEventReader.marker(2), "A failed read kept read ownership")
		withExtendedLifetime(client) {}
	}

	func testCancelledReadReleasesReadOwnership() async throws {
		let reads = HeldReads()
		let (iterator, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()
		first.cancel()
		await reads.releaseRead()
		do {
			_ = try await first.value
			XCTFail("The cancelled read did not throw")
		} catch is CancellationError {}

		let next = try await iterator.next()
		XCTAssertEqual(next, FakeEventReader.marker(2), "A cancelled read kept read ownership")
		withExtendedLifetime(client) {}
	}

	func testReaderEndDuringReadEndsTheIterator() async throws {
		let reads = HeldReads()
		let (iterator, client) = try await makeIterator(reads)
		let first = Task { try await iterator.next() }
		await reads.waitForRead()
		do {
			_ = try await iterator.next()
			XCTFail("A read during the end race was not rejected")
		} catch XmtpError.ConsumerOwned {}

		await reads.end()
		let firstValue = try await first.value
		let next = try await iterator.next()
		XCTAssertNil(firstValue, "The ended reader returned an event")
		XCTAssertNil(next, "The ended iterator returned an event")
		let counts = await reads.counts()
		XCTAssertEqual(counts.reads, 1, "The ended iterator read again")
		withExtendedLifetime(client) {}
	}
}
