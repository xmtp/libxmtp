import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing

struct ConversationLookupLimiterTests {
	@Test func lookupWaitsForSharedReadPermit() async throws {
		let limiter = AsyncLimiter(1), gate = LookupGate()
		let occupying = Task { try await limiter.withPermit { await gate.hold(); return 1 } }
		await gate.waitUntilHeld()
		let queued = Task { try await limitedConversationLookup(limiter: limiter) { await gate.read(); return "found" } }
		try await Task.sleep(for: .milliseconds(50))
		#expect(await gate.reads == 0)
		await gate.release()
		#expect(try await occupying.value == 1)
		#expect(try await queued.value == "found")
		#expect(await gate.reads == 1)
	}

	@Test func canceledQueuedLookupNeverStartsSDKRead() async throws {
		let limiter = AsyncLimiter(1), gate = LookupGate()
		let occupying = Task { try await limiter.withPermit { await gate.hold(); return 1 } }
		await gate.waitUntilHeld()
		let queued = Task { try await limitedConversationLookup(limiter: limiter) { await gate.read(); return "found" } }
		try await Task.sleep(for: .milliseconds(50))
		queued.cancel()
		await gate.release()
		_ = try await occupying.value
		do { _ = try await queued.value; Issue.record("Canceled lookup succeeded") } catch is CancellationError {} catch {
			throw error
		}
		#expect(await gate.reads == 0)
	}
}

private actor LookupGate {
	var reads = 0
	private var held: CheckedContinuation<Void, Never>?
	func hold() async {
		await withCheckedContinuation { held = $0 }
	}

	func waitUntilHeld() async {
		while held == nil {
			await Task.yield()
		}
	}

	func release() {
		held?.resume(); held = nil
	}

	func read() {
		reads += 1
	}
}
