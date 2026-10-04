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
}
