@testable import ExampleCore
import Foundation
import Testing

private actor LimiterProbe {
	var holding = false
	var queuedStarted = false
	var cancelledFinished = false
	var continuation: CheckedContinuation<Void, Never>?
	func hold() async {
		holding = true
		await withCheckedContinuation { continuation = $0 }
	}

	func release() {
		continuation?.resume(); continuation = nil
	}

	func startQueued() {
		queuedStarted = true
	}

	func finishCancelled() {
		cancelledFinished = true
	}
}

struct AsyncLimiterTests {
	@Test func cancelledWaiterFinishesBeforeHeldPermitIsReleased() async throws {
		let limiter = AsyncLimiter(1); let probe = LimiterProbe()
		let holder = Task { try await limiter.withPermit { await probe.hold() } }
		while await !probe.holding {
			await Task.yield()
		}
		let queued = Task {
			await probe.startQueued()
			do {
				_ = try await limiter.withPermit { Issue.record("Cancelled operation ran") }
			} catch is CancellationError {
				await probe.finishCancelled()
			} catch {
				Issue.record("Expected cancellation, got \(error)")
			}
		}
		while await !probe.queuedStarted {
			await Task.yield()
		}
		try await Task.sleep(for: .milliseconds(20))
		queued.cancel()
		for _ in 0 ..< 200 {
			if await probe.cancelledFinished {
				break
			}
			try await Task.sleep(for: .milliseconds(1))
		}
		#expect(await probe.cancelledFinished)
		await probe.release(); try await holder.value; await queued.value
		let next = try await limiter.withPermit { 42 }
		#expect(next == 42)
	}
}
