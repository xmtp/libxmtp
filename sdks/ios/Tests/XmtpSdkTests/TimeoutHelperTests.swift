import Dispatch
import Foundation
import XCTest

/// The `within(seconds:)` and `firstResult(of:_:)` test helpers in
/// `LiveBackend.swift`. They need no backend.
final class TimeoutHelperTests: XCTestCase {
	/// An operation that ignores cancellation does not hold the helper past its
	/// deadline. The helper returns nil and does not wait for the operation.
	func testTimesOutWhenTheOperationIgnoresCancellation() async throws {
		let start = Date()
		let result = try await within(seconds: 0.2) { await ignoreCancellation(seconds: 5) }
		let elapsed = Date().timeIntervalSince(start)
		XCTAssertNil(result, "The helper returned the late result")
		XCTAssertLessThan(elapsed, 2, "The helper waited \(elapsed) seconds for the operation")
	}

	/// A result that comes before the deadline is returned, and an error is thrown.
	func testReturnsTheResultOrErrorBeforeTheDeadline() async throws {
		let value = try await within(seconds: 5) { 7 }
		XCTAssertEqual(value, 7)
		do {
			_ = try await within(seconds: 5) { () async throws -> Int in throw TestFailure("operation failed") }
			XCTFail("The helper did not throw the operation error")
		} catch let failure as TestFailure {
			XCTAssertEqual(failure.description, "operation failed")
		}
	}

	/// When one operation finishes, the other is cancelled. A loser that is not
	/// cancelled would finish its 5 second sleep after the race returned.
	func testCancelsTheLoserWhenTheOtherFinishes() async throws {
		let loser = Shared<String?>(nil)
		let value = try await firstResult(of: { 7 }, {
			do {
				try await pause(seconds: 5)
				loser.update { $0 = "finished" }
			} catch {
				loser.update { $0 = "cancelled" }
			}
			return 0
		})
		XCTAssertEqual(value, 7)
		let cancelled = await eventually(seconds: 2) { loser.value == "cancelled" }
		XCTAssertTrue(cancelled, "The losing operation was not cancelled: \(String(describing: loser.value))")
	}

	/// A cancelled caller ends the race at once with `CancellationError` and
	/// cancels the operation, even when the operation ignores cancellation.
	func testCallerCancellationEndsTheRace() async throws {
		let operationCancelled = Shared(false)
		let started = Shared(false)
		let caller = Task {
			try await within(seconds: 5) {
				await withTaskCancellationHandler {
					started.update { $0 = true }
					return await ignoreCancellation(seconds: 5)
				} onCancel: {
					operationCancelled.update { $0 = true }
				}
			}
		}
		let running = await eventually(seconds: 2) { started.value }
		XCTAssertTrue(running, "The operation did not start")
		let start = Date()
		caller.cancel()
		let result = await caller.result
		let elapsed = Date().timeIntervalSince(start)
		XCTAssertLessThan(elapsed, 2, "The cancelled helper waited \(elapsed) seconds")
		XCTAssertThrowsError(try result.get()) { XCTAssertTrue($0 is CancellationError, "\($0)") }
		XCTAssertTrue(operationCancelled.value, "The operation was not cancelled")
	}

	/// A caller that is cancelled before the race starts gets `CancellationError`
	/// and starts no operation.
	func testCallerCancelledBeforeTheStartStartsNothing() async throws {
		let started = Shared(false)
		let outcome = Shared<Result<Int?, Error>?>(nil)
		Task {
			withUnsafeCurrentTask { $0?.cancel() }
			do {
				let value = try await within(seconds: 0.2) {
					started.update { $0 = true }
					return 7
				}
				outcome.update { $0 = .success(value) }
			} catch {
				outcome.update { $0 = .failure(error) }
			}
		}
		let ended = await eventually(seconds: 2) { outcome.value != nil }
		XCTAssertTrue(ended, "The cancelled helper did not return")
		XCTAssertThrowsError(try outcome.value?.get()) { XCTAssertTrue($0 is CancellationError, "\($0)") }
		XCTAssertFalse(started.value, "A cancelled caller started the operation")
	}
}

/// Returns after `seconds` and does not stop when its task is cancelled.
private func ignoreCancellation(seconds: Double) async -> Int {
	await withCheckedContinuation { continuation in
		DispatchQueue.global().asyncAfter(deadline: .now() + seconds) { continuation.resume(returning: 1) }
	}
}
