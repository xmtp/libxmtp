import Foundation
import SwiftProtobuf
import XCTest
@testable import XMTPiOS

@available(iOS 16, *)
final class MessageDeliveryStreamTests: XCTestCase {
	private enum TestError: Error, Equatable {
		case acknowledgement
	}

	private enum TestContent {
		case text
		case forgedMembership
		case malformed
	}

	private final class Token: MessageDeliveryToken, @unchecked Sendable {
		struct Counts {
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

	private final class NativeToken: FfiDeliveryAcknowledgement, @unchecked Sendable {
		var token = Token()

		override func checkOwner() throws -> Bool {
			try token.checkOwner()
		}

		override func acknowledge() throws {
			try token.acknowledge()
		}

		override func reject() {
			token.reject()
		}
	}

	private final class NativeCloser: FfiStreamCloser, @unchecked Sendable {
		var onEnd: (() -> Void)?

		override func end() {
			onEnd?()
		}
	}

	private func delivery(
		_ id: UInt8 = 1,
		token: Token,
		content: TestContent = .text
	) throws -> QueuedMessageDelivery {
		let bytes: Data = switch content {
		case .text:
			try TextCodec().encode(content: "message \(id)").serializedData()
		case .forgedMembership:
			try GroupUpdatedCodec().encode(content: GroupUpdated()).serializedData()
		case .malformed:
			Data([0xFF])
		}
		return QueuedMessageDelivery(
			message: FfiMessage(
				id: Data([id]), sentAtNs: 1, conversationId: Data(repeating: 1, count: 16),
				senderInboxId: "sender", content: bytes, kind: .application,
				deliveryStatus: .published, sequenceId: UInt64(id), insertedAtNs: 1,
				expireAtNs: nil
			),
			cursor: FfiDeliveryCursor(databaseId: Data(repeating: 7, count: 16), deliverySequence: UInt64(id)),
			acknowledgement: token
		)
	}

	func testReceiveAndFirstNextDoNotAcknowledgeButSecondNextDoes() async throws {
		for includeFiltered in [false, true] {
			let acknowledged = expectation(description: "first item acknowledged")
			let first = Token(onAcknowledgement: { acknowledged.fulfill() })
			let second = Token()
			let stream = MessageDeliveryStream(onClose: nil)
			defer { stream.finish() }
			let initial: DecodedMessage?
			if includeFiltered {
				let filteredAcknowledged = expectation(description: "filtered item acknowledged")
				let filtered = Token(onAcknowledgement: { filteredAcknowledged.fulfill() })
				try stream.receive(delivery(0, token: filtered, content: .forgedMembership))
				XCTAssertEqual(filtered.counts().acknowledgements, 0)
				let next = Task { try await stream.next() }
				defer { next.cancel() }
				await fulfillment(of: [filteredAcknowledged], timeout: 3)
				XCTAssertEqual(filtered.counts().acknowledgements, 1)
				XCTAssertEqual(filtered.counts().rejections, 0)
				try stream.receive(delivery(token: first))
				initial = try await next.value
			} else {
				try stream.receive(delivery(token: first))
				XCTAssertEqual(first.counts().acknowledgements, 0)
				initial = try await stream.next()
			}
			XCTAssertEqual(initial?.id, "01")
			XCTAssertEqual(try initial?.content() as String?, "message 1")
			XCTAssertEqual(initial?.deliveryCursor?.deliverySequence, 1)
			XCTAssertEqual(first.counts().acknowledgements, 0)

			let next = Task { try await stream.next() }
			defer { next.cancel() }
			await fulfillment(of: [acknowledged], timeout: 3)
			try stream.receive(delivery(2, token: second))
			let following = try await next.value
			XCTAssertEqual(following?.id, "02")
			XCTAssertEqual(try following?.content() as String?, "message 2")
			XCTAssertEqual(following?.deliveryCursor?.deliverySequence, 2)
			XCTAssertEqual(first.counts().acknowledgements, 1)
			XCTAssertEqual(second.counts().acknowledgements, 0)
		}
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
		XCTAssertEqual(queued.counts().acknowledgements, 0)
		await fulfillment(of: [closed], timeout: 3)
	}

	func testDroppingTheFullStreamRejectsPendingItemsAndClosesTheSubscription() async throws {
		for consume in [false, true] {
			let received = expectation(description: "delivery received")
			let closed = expectation(description: "stream closed once")
			let ended = expectation(description: "subscription ended once")
			closed.assertForOverFulfill = true
			ended.assertForOverFulfill = true
			let nativeToken = NativeToken(noHandle: .init())
			let pending = try delivery(token: nativeToken.token)
			let nativeCloser = NativeCloser(noHandle: .init())
			nativeCloser.onEnd = { ended.fulfill() }
			let holder = StreamHolder()
			var stream: AsyncThrowingStream<DecodedMessage, Error>? = messageDeliveryStream(
				holder: holder,
				onClose: { closed.fulfill() }
			) { callback in
				do {
					try callback.onMessage(delivery: FfiMessageDelivery(
						message: pending.message, cursor: pending.cursor,
						acknowledgement: nativeToken
					))
				} catch {
					XCTFail("Message callback failed: \(error)")
				}
				received.fulfill()
				return nativeCloser
			}
			var iterator = stream?.makeAsyncIterator()
			await fulfillment(of: [received], timeout: 3)
			if consume {
				let first = try await iterator?.next()
				XCTAssertEqual(first?.id, "01")
			}
			stream = nil
			XCTAssertEqual(nativeToken.token.counts().rejections, 0)
			iterator = nil
			await fulfillment(of: [closed, ended], timeout: 3)
			holder.end()
			XCTAssertEqual(nativeToken.token.counts().acknowledgements, 0)
			XCTAssertEqual(nativeToken.token.counts().rejections, 1)
		}
	}

	func testCancellationBeforeHandoffRejectsTheItem() async throws {
		for content in [TestContent.text, .forgedMembership] {
			let token = Token(onCheck: {
				withUnsafeCurrentTask { $0?.cancel() }
			})
			let stream = MessageDeliveryStream(onClose: nil)
			defer { stream.finish() }
			try stream.receive(delivery(token: token, content: content))
			let next = Task { try await stream.next() }
			defer { next.cancel() }
			try await assertThrowsAsyncError(await next.value) { error in
				XCTAssertTrue(error is CancellationError)
			}
			XCTAssertEqual(token.counts().rejections, 1)
			XCTAssertEqual(token.counts().acknowledgements, 0)
		}
	}

	func testSelectionChangeRejectsTheStaleItemAndWaitsForFreshSelection() async throws {
		for content in [TestContent.text, .forgedMembership] {
			let rejected = expectation(description: "stale item rejected")
			let stale = Token(current: false, onReject: { rejected.fulfill() })
			let fresh = Token()
			let stream = MessageDeliveryStream(onClose: nil)
			defer { stream.finish() }
			try stream.receive(delivery(token: stale, content: content))
			let next = Task { try await stream.next() }
			defer { next.cancel() }
			await fulfillment(of: [rejected], timeout: 3)
			try stream.receive(delivery(2, token: fresh))
			let selected = try await next.value
			XCTAssertEqual(selected?.id, "02")
			XCTAssertEqual(try selected?.content() as String?, "message 2")
			XCTAssertEqual(selected?.deliveryCursor?.deliverySequence, 2)
			XCTAssertEqual(stale.counts().acknowledgements, 0)
			XCTAssertEqual(stale.counts().rejections, 1)
			XCTAssertEqual(fresh.counts().acknowledgements, 0)
		}
	}

	func testFinishDuringOwnershipCheckPreventsHandoff() async throws {
		for content in [TestContent.text, .forgedMembership] {
			let stream = MessageDeliveryStream(onClose: nil)
			let token = Token(onCheck: { [weak stream] in stream?.finish() })
			try stream.receive(delivery(token: token, content: content))
			try await assertThrowsAsyncError(await stream.next()) { error in
				XCTAssertTrue(error is CancellationError)
			}
			XCTAssertEqual(token.counts().acknowledgements, 0)
			XCTAssertEqual(token.counts().rejections, 1)
		}
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
		defer { stream.finish() }
		let malformed = try delivery(token: token, content: .malformed)
		let valid = try delivery(2, token: later)
		stream.receive(malformed)
		try await assertThrowsAsyncError(await stream.next()) { error in
			XCTAssertTrue(error is BinaryDecodingError)
		}
		stream.receive(valid)
		XCTAssertEqual(token.counts().acknowledgements, 0)
		XCTAssertEqual(token.counts().rejections, 1)
		XCTAssertEqual(later.counts().acknowledgements, 0)
		XCTAssertEqual(later.counts().rejections, 1)

		let forged = try delivery(0, token: Token(), content: .forgedMembership)
		let snapshotCursor = FfiDeliveryCursor(
			databaseId: valid.cursor.databaseId, deliverySequence: 3
		)
		let snapshot = try MessageHistorySnapshot(FfiMessageHistorySnapshot(
			messages: [forged, valid].map { FfiHistoryMessage(message: $0.message, cursor: $0.cursor) },
			cursor: snapshotCursor
		))
		XCTAssertEqual(snapshot.messages.map(\.id), ["02"])
		XCTAssertEqual(try snapshot.messages.map { try $0.content() as String }, ["message 2"])
		XCTAssertEqual(snapshot.messages.first?.deliveryCursor, valid.cursor)
		XCTAssertEqual(snapshot.cursor, snapshotCursor)
		XCTAssertThrowsError(try MessageHistorySnapshot(FfiMessageHistorySnapshot(
			messages: [forged, malformed, valid].map { FfiHistoryMessage(message: $0.message, cursor: $0.cursor) },
			cursor: snapshotCursor
		))) { error in
			XCTAssertTrue(error is BinaryDecodingError)
		}
	}

	func testAcknowledgementFailureRejectsBothItemsAndStops() async throws {
		for content in [TestContent.text, .forgedMembership] {
			let queued = Token()
			let pending = try delivery(2, token: queued)
			let stream = MessageDeliveryStream(onClose: nil)
			defer { stream.finish() }
			let first = Token(acknowledgementFails: true, onCheck: { [weak stream] in
				stream?.receive(pending)
			})
			try stream.receive(delivery(token: first, content: content))
			if case .text = content {
				let initial = try await stream.next()
				XCTAssertEqual(initial?.id, "01")
			}
			try await assertThrowsAsyncError(await stream.next()) { error in
				XCTAssertEqual(error as? TestError, .acknowledgement)
			}
			XCTAssertEqual(first.counts().rejections, 1)
			XCTAssertEqual(first.counts().acknowledgements, 0)
			XCTAssertEqual(queued.counts().acknowledgements, 0)
			XCTAssertEqual(queued.counts().rejections, 1)
		}
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
		XCTAssertEqual(first.counts().rejections, 1)
		XCTAssertEqual(second.counts().rejections, 1)
		XCTAssertEqual(first.counts().acknowledgements, 0)
		XCTAssertEqual(second.counts().acknowledgements, 0)
	}
}
