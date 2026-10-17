import OSLog
import XCTest
@testable import XmtpSdk

final class AppleLifecycleTests: XCTestCase {
	private enum ResumeFailure: Error {
		case expected
	}

	private actor Calls {
		private var suspends = 0
		private var resumes = 0

		func suspend() {
			suspends += 1
		}

		func resume() throws {
			resumes += 1
			if resumes == 1 {
				throw ResumeFailure.expected
			}
		}

		func counts() -> (suspends: Int, resumes: Int) {
			(suspends, resumes)
		}
	}

	func testEnableRetriesFailedForegroundResume() async {
		let calls = Calls()
		let manager = StreamLifecycleManager(
			suspend: { await calls.suspend() },
			resume: { try await calls.resume() },
		)
		await manager.setDesired(live: false)?.value
		await manager.setDesired(live: true)?.value
		let failed = await calls.counts()
		XCTAssertEqual(failed.suspends, 1)
		XCTAssertEqual(failed.resumes, 1)

		await manager.enableIfNeeded()
		let retried = await calls.counts()
		XCTAssertEqual(retried.suspends, 1)
		XCTAssertEqual(retried.resumes, 2)

		await manager.enableIfNeeded()
		let stable = await calls.counts()
		XCTAssertEqual(stable.resumes, 2)
	}

	func testEnableKeepsBackgroundStreamsSuspended() async {
		let calls = Calls()
		let manager = StreamLifecycleManager(
			suspend: { await calls.suspend() },
			resume: { try await calls.resume() },
		)
		await manager.setDesired(live: false)?.value
		await manager.enableIfNeeded()
		let result = await calls.counts()
		XCTAssertEqual(result.suspends, 1)
		XCTAssertEqual(result.resumes, 0)
	}

	private struct BackendFailure: LocalizedError {
		var errorDescription: String? {
			"backend rejected credential-lifecycle-secret"
		}
	}

	/// The resume failure log names the operation. It does not contain the error text.
	func testFailedResumeLogOmitsErrorText() async throws {
		guard #available(macOS 12, iOS 15, *) else {
			throw XCTSkip("OSLogStore needs macOS 12 or iOS 15")
		}
		let store = try OSLogStore(scope: .currentProcessIdentifier)
		let start = store.position(date: Date())
		let manager = StreamLifecycleManager(suspend: {}, resume: { throw BackendFailure() })
		await manager.setDesired(live: false)?.value
		await manager.setDesired(live: true)?.value

		// A log entry can reach the store after a short delay. One read can take seconds.
		var messages: [String] = []
		for _ in 0 ..< 20 {
			messages = try store.getEntries(at: start)
				.compactMap { $0 as? OSLogEntryLog }
				.map(\.composedMessage)
			if messages.contains(where: { $0.contains("Stream resume failed") }) {
				break
			}
			try await Task.sleep(nanoseconds: 100_000_000)
		}
		XCTAssertTrue(messages.contains { $0.contains("Stream resume failed") }, "The failed resume did not log")
		XCTAssertFalse(
			messages.contains { $0.contains("credential-lifecycle-secret") },
			"The lifecycle log contains the error text",
		)
	}

	#if canImport(UIKit)
		func testManageStreamLifecycleDefaultsOn() {
			XCTAssertTrue(SDKClient.manageStreamLifecycle)
		}
	#endif
}
