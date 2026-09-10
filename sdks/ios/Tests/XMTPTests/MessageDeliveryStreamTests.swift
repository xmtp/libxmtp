import Foundation
import XCTest
@testable import XMTPiOS

@available(iOS 16, *)
final class MessageDeliveryStreamTests: XCTestCase {
	private enum TestError: Error, Equatable {
		case acknowledgement
	}

	private final class Token: MessageDeliveryToken, @unchecked Sendable {
		struct Counts {
			var checks = 0
			var acknowledgements = 0
			var rejections = 0
		}

		private let lock = NSLock()
		private var state = Counts()
		private let current: Bool
		private let acknowledgementFails: Bool
		private let onCheck: (() -> Void)?
		private let onAcknowledgement: (() -> Void)?
		private let onReject: (() -> Void)?

		init(
			current: Bool = true,
			acknowledgementFails: Bool = false,
			onCheck: (() -> Void)? = nil,
			onAcknowledgement: (() -> Void)? = nil,
			onReject: (() -> Void)? = nil
		) {
			self.current = current
			self.acknowledgementFails = acknowledgementFails
			self.onCheck = onCheck
			self.onAcknowledgement = onAcknowledgement
			self.onReject = onReject
		}

		func counts() -> Counts {
			lock.lock()
			defer { lock.unlock() }
			return state
		}

		func checkOwner() throws -> Bool {
			lock.lock()
			state.checks += 1
			lock.unlock()
			onCheck?()
			return current
		}

		func acknowledge() throws {
			if acknowledgementFails {
				throw TestError.acknowledgement
			}
			lock.lock()
			state.acknowledgements += 1
			lock.unlock()
			onAcknowledgement?()
		}

		func reject() {
			lock.lock()
			state.rejections += 1
			lock.unlock()
			onReject?()
		}
	}

	private func delivery(
		_ id: UInt8 = 1,
		token: Token,
		validContent: Bool = true
	) throws -> QueuedMessageDelivery {
		let content: Data = if validContent {
			try TextCodec().encode(content: "message \(id)").serializedData()
		} else {
			Data([0xFF])
		}
		return QueuedMessageDelivery(
			message: FfiMessage(
				id: Data([id]), sentAtNs: 1, conversationId: Data(repeating: 1, count: 16),
				senderInboxId: "sender", content: content, kind: .application,
				deliveryStatus: .published, sequenceId: UInt64(id), insertedAtNs: 1,
				expireAtNs: nil
			),
			cursor: FfiDeliveryCursor(databaseId: Data(repeating: 7, count: 16), deliverySequence: UInt64(id)),
			acknowledgement: token
		)
	}

	func testReceiveAndFirstNextDoNotAcknowledgeButSecondNextDoes() async throws {
		let acknowledged = expectation(description: "first item acknowledged")
		let first = Token(onAcknowledgement: { acknowledged.fulfill() })
		let second = Token()
		let stream = MessageDeliveryStream(onClose: nil)
		defer { stream.finish() }
		try stream.receive(delivery(token: first))
		XCTAssertEqual(first.counts().checks, 0)
		XCTAssertEqual(first.counts().acknowledgements, 0)
		let initial = try await stream.next()
		XCTAssertEqual(initial?.deliveryCursor?.deliverySequence, 1)
		XCTAssertEqual(first.counts().checks, 1)
		XCTAssertEqual(first.counts().acknowledgements, 0)

		let next = Task { try await stream.next() }
		defer { next.cancel() }
		await fulfillment(of: [acknowledged], timeout: 3)
		try stream.receive(delivery(2, token: second))
		let following = try await next.value
		XCTAssertEqual(following?.deliveryCursor?.deliverySequence, 2)
		XCTAssertEqual(first.counts().acknowledgements, 1)
		XCTAssertEqual(second.counts().acknowledgements, 0)
	}

	func testFinishRejectsTheLastAndQueuedItemsAndClosesOnce() async throws {
		let closed = expectation(description: "closed once")
		closed.assertForOverFulfill = true
		let first = Token()
		let queued = Token()
		let stream = MessageDeliveryStream(onClose: { closed.fulfill() })
		try stream.receive(delivery(token: first))
		_ = try await stream.next()
		try stream.receive(delivery(2, token: queued))
		stream.finish()
		stream.finish()
		let ended = try await stream.next()
		XCTAssertNil(ended)
		XCTAssertEqual(first.counts().acknowledgements, 0)
		XCTAssertEqual(first.counts().rejections, 1)
		XCTAssertEqual(queued.counts().rejections, 1)
		await fulfillment(of: [closed], timeout: 3)
	}

	func testDroppingTheLastStreamIteratorRejectsTheLastItem() async throws {
		let token = Token()
		let pending = try delivery(token: token)
		weak var receipt: MessageDeliveryStream?
		func makeIterator() -> AsyncThrowingStream<DecodedMessage, Error>.Iterator {
			let stream = MessageDeliveryStream(onClose: nil)
			receipt = stream
			stream.receive(pending)
			return AsyncThrowingStream(unfolding: { try await stream.next() }).makeAsyncIterator()
		}
		var iterator: AsyncThrowingStream<DecodedMessage, Error>.Iterator? = makeIterator()
		let first = try await iterator?.next()
		XCTAssertNotNil(first)
		iterator = nil
		XCTAssertNil(receipt)
		XCTAssertEqual(token.counts().acknowledgements, 0)
		XCTAssertEqual(token.counts().rejections, 1)
	}

	func testDroppingTheMailboxRejectsAnUnconsumedItem() throws {
		let token = Token()
		var stream: MessageDeliveryStream? = MessageDeliveryStream(onClose: nil)
		weak var receipt = stream
		try stream?.receive(delivery(token: token))
		stream = nil
		XCTAssertNil(receipt)
		XCTAssertEqual(token.counts().rejections, 1)
		XCTAssertEqual(token.counts().acknowledgements, 0)
	}

	func testCancellationBeforeHandoffRejectsTheItem() async throws {
		let token = Token(onCheck: {
			withUnsafeCurrentTask { $0?.cancel() }
		})
		let stream = MessageDeliveryStream(onClose: nil)
		try stream.receive(delivery(token: token))
		let next = Task { try await stream.next() }
		try await assertThrowsAsyncError(await next.value) { error in
			XCTAssertTrue(error is CancellationError)
		}
		XCTAssertEqual(token.counts().rejections, 1)
		XCTAssertEqual(token.counts().acknowledgements, 0)
	}

	func testSelectionChangeRejectsTheStaleItemAndWaitsForFreshSelection() async throws {
		let rejected = expectation(description: "stale item rejected")
		let stale = Token(current: false, onReject: { rejected.fulfill() })
		let fresh = Token()
		let stream = MessageDeliveryStream(onClose: nil)
		defer { stream.finish() }
		try stream.receive(delivery(token: stale))
		let next = Task { try await stream.next() }
		defer { next.cancel() }
		await fulfillment(of: [rejected], timeout: 3)
		try stream.receive(delivery(2, token: fresh))
		let selected = try await next.value
		XCTAssertEqual(selected?.deliveryCursor?.deliverySequence, 2)
		XCTAssertEqual(stale.counts().acknowledgements, 0)
		XCTAssertEqual(stale.counts().rejections, 1)
		XCTAssertEqual(fresh.counts().acknowledgements, 0)
	}

	func testFinishDuringOwnershipCheckPreventsHandoff() async throws {
		let stream = MessageDeliveryStream(onClose: nil)
		let token = Token(onCheck: { [weak stream] in stream?.finish() })
		try stream.receive(delivery(token: token))
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertTrue(error is CancellationError)
		}
		XCTAssertEqual(token.counts().acknowledgements, 0)
		XCTAssertEqual(token.counts().rejections, 1)
	}

	func testConcurrentNextStopsBothCallsWithoutAcknowledgement() async throws {
		let rejected = expectation(description: "first next is active")
		let stale = Token(current: false, onReject: { rejected.fulfill() })
		let stream = MessageDeliveryStream(onClose: nil)
		defer { stream.finish() }
		try stream.receive(delivery(token: stale))
		let first = Task { try await stream.next() }
		defer { first.cancel() }
		await fulfillment(of: [rejected], timeout: 3)
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertEqual(error as? MessageDeliveryStreamError, .concurrentNext)
		}
		try await assertThrowsAsyncError(await first.value) { error in
			XCTAssertEqual(error as? MessageDeliveryStreamError, .concurrentNext)
		}
		XCTAssertEqual(stale.counts().acknowledgements, 0)
	}

	func testDecodeFailureRejectsTheItemAndStops() async throws {
		let token = Token()
		let later = Token()
		let stream = MessageDeliveryStream(onClose: nil)
		try stream.receive(delivery(token: token, validContent: false))
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertEqual(error as? MessageDeliveryStreamError, .decodeFailed)
		}
		try stream.receive(delivery(2, token: later))
		XCTAssertEqual(token.counts().checks, 0)
		XCTAssertEqual(token.counts().rejections, 1)
		XCTAssertEqual(later.counts().rejections, 1)
	}

	func testAcknowledgementFailureRejectsBothItemsAndStops() async throws {
		let first = Token(acknowledgementFails: true)
		let queued = Token()
		let stream = MessageDeliveryStream(onClose: nil)
		try stream.receive(delivery(token: first))
		_ = try await stream.next()
		try stream.receive(delivery(2, token: queued))
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertEqual(error as? TestError, .acknowledgement)
		}
		XCTAssertEqual(first.counts().rejections, 1)
		XCTAssertEqual(first.counts().acknowledgements, 0)
		XCTAssertEqual(queued.counts().checks, 0)
		XCTAssertEqual(queued.counts().rejections, 1)
	}

	func testOneSlotOverflowRejectsBothItemsWithoutHandoff() async throws {
		let first = Token()
		let second = Token()
		let stream = MessageDeliveryStream(onClose: nil)
		try stream.receive(delivery(token: first))
		try stream.receive(delivery(2, token: second))
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertEqual(error as? MessageDeliveryStreamError, .queueFull)
		}
		XCTAssertEqual(first.counts().checks, 0)
		XCTAssertEqual(first.counts().rejections, 1)
		XCTAssertEqual(second.counts().rejections, 1)
	}
}
