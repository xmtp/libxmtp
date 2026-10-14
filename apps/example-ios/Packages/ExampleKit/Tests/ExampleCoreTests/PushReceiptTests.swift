@testable import ExampleCore
import Foundation
import Testing

@MainActor struct PushReceiptTests {
	private func envelope(_ sequence: UInt64) throws -> PushEnvelope {
		try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(),
			sequence: String(sequence),
		))
	}

	@Test func pendingPostDoesNotConsumeReceipt() async throws {
		let fixture = PushFixture()
		fixture.publisher.held = true
		let value = try envelope(1)
		let task = Task { await fixture.controller.receive(value, lease: PushLease()) }
		while fixture.publisher.completion == nil {
			await Task.yield()
		}
		#expect(!fixture.preferences.hasPosted(value.tag))
		#expect(await fixture.controller.receive(value, lease: PushLease()) == false)
		fixture.publisher.complete()
		#expect(await task.value)
		#expect(fixture.preferences.hasPosted(value.tag))
	}

	@Test func rejectedPostCanRetry() async throws {
		let fixture = PushFixture()
		fixture.publisher.held = true
		let value = try envelope(2)
		let task = Task { await fixture.controller.receive(value, lease: PushLease()) }
		while fixture.publisher.completion == nil {
			await Task.yield()
		}
		fixture.publisher.completion?(false)
		fixture.publisher.completion = nil
		#expect(await task.value == false)
		#expect(!fixture.preferences.hasPosted(value.tag))
		fixture.publisher.held = false
		#expect(await fixture.controller.receive(value, lease: PushLease()))
	}

	@Test func receiptsRemainBoundedWithoutReenablingRetiredReplays() throws {
		var receipts = PushReceiptWindow()
		for number in 1 ... 4096 {
			try receipts.record(envelope(UInt64(number * 2)).tag)
		}
		let data = try JSONEncoder().encode(receipts)
		#expect(data.count < 8192)
		var restored = try JSONDecoder().decode(PushReceiptWindow.self, from: data)
		#expect(try restored.contains(envelope(2).tag))
		#expect(try restored.contains(envelope(8192).tag))
		#expect(try !restored.contains(envelope(8191).tag))
		try restored.record(envelope(8191).tag)
		#expect(try restored.contains(envelope(8191).tag))
		// A never-seen notification below the retired floor is intentionally stale.
		#expect(try restored.contains(envelope(1).tag))
		#expect(try !restored.contains(envelope(UInt64.max).tag))
	}
}
