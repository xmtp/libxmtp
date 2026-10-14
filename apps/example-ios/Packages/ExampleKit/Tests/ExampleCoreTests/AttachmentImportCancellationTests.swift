@testable import ExampleCore
import Foundation
import Testing

private actor ImportCancellationProbe {
	var holding = false
	var started = 0
	var cancelledFinished = 0
	var survivorRan = false
	private var continuation: CheckedContinuation<Void, Never>?

	func hold() async {
		holding = true
		await withCheckedContinuation { continuation = $0 }
	}

	func release() {
		continuation?.resume(); continuation = nil
	}

	func start() {
		started += 1
	}

	func finishCancellation() {
		cancelledFinished += 1
	}

	func runSurvivor() {
		survivorRan = true
	}
}

struct RoundFifteenAttachmentImportTests {
	@Test(arguments: [1, 4]) func cancelledQueuedImportsFinishWithoutReleasingActiveTransfer(count: Int) async throws {
		let gate = AttachmentImportGate<String>()
		let probe = ImportCancellationProbe()
		let holder = Task {
			try await gate.withContext("active-profile") {
				await probe.hold()
				return await gate.current
			}
		}
		while await !probe.holding {
			await Task.yield()
		}
		let queued = (0 ..< count).map { _ in Task {
			await probe.start()
			do {
				_ = try await gate.withContext("cancelled-profile") { Issue.record("Cancelled importer ran") }
			} catch is CancellationError {
				await probe.finishCancellation()
			} catch {
				Issue.record("Expected cancellation, got \(error)")
			}
		} }
		while await probe.started < count {
			await Task.yield()
		}
		try await Task.sleep(for: .milliseconds(20))
		queued.forEach { $0.cancel() }
		let deadline = ContinuousClock.now.advanced(by: .milliseconds(500))
		while await probe.cancelledFinished < count, ContinuousClock.now < deadline {
			try await Task.sleep(for: .milliseconds(1))
		}
		#expect(await probe.cancelledFinished == count)
		#expect(await gate.current == "active-profile")
		let survivor = Task {
			try await gate.withContext("next-profile") {
				await probe.runSurvivor()
				return await gate.current
			}
		}
		try await Task.sleep(for: .milliseconds(20))
		#expect(await !probe.survivorRan)
		#expect(await gate.current == "active-profile")
		await probe.release()
		#expect(try await holder.value == "active-profile")
		for task in queued {
			await task.value
		}
		#expect(try await survivor.value == "next-profile")
		#expect(await gate.current == nil)
		#expect(try await gate.withContext("fresh-profile") { await gate.current } == "fresh-profile")
	}
}
