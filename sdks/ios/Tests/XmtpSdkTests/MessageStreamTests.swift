import Foundation
import XCTest
import XmtpSdk

/// The Swift stream adapter (`runtime/streams/Readers.swift`). Rust tests prove
/// the reader cursor and ACK rules; these tests prove that the Swift iterator
/// asks for them at the correct time.
final class MessageStreamTests: XCTestCase {
	/// A new iterator request acknowledges the previous message. A loop break
	/// and a dropped iterator acknowledge nothing, and a cancelled idle read
	/// returns.
	func testNextRequestAcknowledgesThePreviousMessage() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let firstId = try await group.sendText(text: "first")

		let breakClose = CloseSignal()
		for try await message in try await client.messages(in: group, onClose: breakClose.record) {
			XCTAssertEqual(message.id, firstId)
			break
		}
		try await breakClose.wait()
		let afterBreak = try await firstUnacknowledged(group)
		XCTAssertEqual(afterBreak, firstId, "A loop break acknowledged the message")

		let secondId = try await group.sendText(text: "second")
		let dropClose = CloseSignal()
		var iterator: SDKMessageStream.Iterator? = try await client.messages(
			in: group, onClose: dropClose.record,
		).makeAsyncIterator()
		let first = try await read(iterator)
		XCTAssertEqual(first?.id, firstId)
		let second = try await read(iterator)
		XCTAssertEqual(second?.id, secondId)
		iterator = nil
		try await dropClose.wait()
		let afterDrop = try await firstUnacknowledged(group)
		XCTAssertEqual(afterDrop, secondId, "The second request did not acknowledge only the first message")

		let idleClose = CloseSignal()
		let idleIterator = try await client.messages(in: group, onClose: idleClose.record).makeAsyncIterator()
		let delivered = try await read(idleIterator)
		XCTAssertEqual(delivered?.id, secondId)
		let idle = Task { try await idleIterator.next() }
		try await Task.sleep(for: .milliseconds(50))
		idle.cancel()
		let reason = try await idleClose.wait()
		guard case .closed = reason else {
			return XCTFail("A cancelled idle read reported \(reason)")
		}
		_ = try? await idle.value
		try await client.end()
	}

	/// The ID that a new raw reader delivers first. The reader ends without an
	/// acknowledgement.
	private func firstUnacknowledged(_ group: Group) async throws -> MessageId? {
		let reader = try await group.messageReader()
		let message = try await within(seconds: 30) { try await reader.next() }
		try await reader.end()
		return message??.id
	}

	/// The next message, or a test failure when none arrives in 30 seconds.
	private func read(_ iterator: SDKMessageStream.Iterator?) async throws -> Message? {
		guard let iterator else { return nil }
		guard let message = try await within(seconds: 30, { try await iterator.next() }) else {
			throw TestFailure("the stream delivered no message in 30 seconds")
		}
		return message
	}

	/// A loop break closes the stream with `.closed`. A close callback that
	/// throws still receives `.closed`, and the stream does not fail.
	func testBreakReportsClosedEvenWhenTheCallbackThrows() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let messageId = try await group.sendText(text: "close after break")
		let throwing = CloseSignal(throwing: true)
		for try await message in try await client.messages(in: group, onClose: throwing.record) {
			XCTAssertEqual(message.id, messageId)
			break
		}
		let reason = try await throwing.wait()
		guard case .closed = reason else {
			return XCTFail("A loop break reported \(reason)")
		}
		XCTAssertEqual(throwing.count, 1)
		try await client.end()
	}

	/// A transport failure does not end the Swift stream. The stream reports the
	/// outage, and after the network returns it delivers the message that was
	/// sent during the outage.
	func testStreamRecoversAfterTheConnectionDrops() async throws {
		let relay = try TestRelay(backend: liveBackendURL)
		let relayURL = try await relay.start()
		defer { relay.close() }
		let receiver = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions(url: relayURL))
		let sender = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let group = try await sender.conversations().createGroup(members: [receiver.inboxId()])
		try await receiver.conversations().sync()
		guard case let .group(joined)? = try await receiver.conversations().getById(id: group.id()) else {
			return XCTFail("The receiver did not join the group")
		}
		let states = Shared<[ConnectionState]>([])
		let closed = CloseSignal()
		let iterator = try await receiver.messages(
			in: joined, onClose: closed.record,
			onConnectionStateChange: { _, current in states.update { $0.append(current) } },
		).makeAsyncIterator()
		func readUntil(_ id: MessageId, seconds: Double) async throws -> Bool {
			let found = try await within(seconds: seconds) {
				while let message = try await iterator.next() {
					if message.id == id {
						return true
					}
				}
				return false
			}
			return found == true
		}

		let beforeId = try await group.sendText(text: "before the outage")
		let readBefore = try await readUntil(beforeId, seconds: 30)
		XCTAssertTrue(readBefore, "The stream did not deliver the first message")
		let connected = await eventually(seconds: 30) { states.value.last == .connected }
		XCTAssertTrue(connected, "The stream did not connect: \(states.value)")

		relay.disconnect()
		let duringId = try await group.sendText(text: "during the outage")
		let interrupted = await eventually(seconds: 30) { states.value.last != .connected }
		XCTAssertTrue(interrupted, "The stream did not report the outage: \(states.value)")
		relay.restore()
		let readDuring = try await readUntil(duringId, seconds: 120)
		XCTAssertTrue(readDuring, "The stream did not deliver the message sent during the outage: \(states.value)")
		XCTAssertEqual(closed.count, 0, "The outage closed the stream")
		let reconnected = await eventually(seconds: 30) { states.value.last == .connected }
		XCTAssertTrue(reconnected, "The stream did not report the recovered connection: \(states.value)")

		try await sender.end()
		try await receiver.end()
	}
}

/// Records the close reasons of one stream.
final class CloseSignal: @unchecked Sendable {
	private enum CallbackFailure: Error { case expected }

	private let reasons = Shared<[SDKStreamCloseReason]>([])
	private let throwing: Bool

	init(throwing: Bool = false) {
		self.throwing = throwing
	}

	var record: @Sendable (SDKStreamCloseReason) throws -> Void {
		{ [self] reason in
			reasons.update { $0.append(reason) }
			if throwing {
				throw CallbackFailure.expected
			}
		}
	}

	var count: Int {
		reasons.value.count
	}

	/// The first close reason. Fails when the stream does not close in 10 seconds.
	@discardableResult
	func wait() async throws -> SDKStreamCloseReason {
		guard await eventually(seconds: 10, { !reasons.value.isEmpty }) else {
			throw TestFailure("the stream did not close")
		}
		return reasons.value[0]
	}
}
