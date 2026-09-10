import XCTest
@testable import XMTPiOS

final class StreamFailureTests: XCTestCase {
	func testPassesTheRawNativeErrorMessageToTheTypedDecoder() {
		let message = "[BarrierError::Incomplete] failed\n[XMTP_STREAM_FAILURE_V1]{}"
		let details = StreamFailureDetails(
			kind: .barrier,
			code: "BarrierError::Incomplete",
			message: "Processing barriers did not complete",
			retryable: true,
			intentId: nil,
			publishedIntentIds: [],
			summary: nil,
			barriers: []
		)
		var decodedMessage: String?
		let result = readStreamFailureDetails(FfiError.Error(message: message)) {
			decodedMessage = $0
			return details
		}
		XCTAssertEqual(decodedMessage, message)
		XCTAssertEqual(result, details)
	}

	func testDoesNotDecodeUnrelatedErrors() {
		struct OrdinaryError: Error {}
		var called = false
		let result = readStreamFailureDetails(OrdinaryError()) { _ in
			called = true
			return nil
		}
		XCTAssertNil(result)
		XCTAssertFalse(called)
	}
}
