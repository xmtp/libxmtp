import Dispatch
import Foundation
import XCTest

/// The `within(seconds:)` test helper in `LiveBackend.swift`. It needs no backend.
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
}

/// Returns after `seconds` and does not stop when its task is cancelled.
private func ignoreCancellation(seconds: Double) async -> Int {
	await withCheckedContinuation { continuation in
		DispatchQueue.global().asyncAfter(deadline: .now() + seconds) { continuation.resume(returning: 1) }
	}
}
