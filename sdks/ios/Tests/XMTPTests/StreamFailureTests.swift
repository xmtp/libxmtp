import XCTest
@testable import XMTPiOS

final class StreamFailureTests: XCTestCase {
	func testReadsTypedDetailsFromThePublicErrorProperty() throws {
		let message = "[BarrierError::Incomplete] failed\n[XMTP_STREAM_FAILURE_V1]" + """
		{"kind":"barrier","code":"BarrierError::Incomplete",
		 "message":"Processing barriers did not complete","retryable":true,
		 "intentId":null,"publishedIntentIds":[],"summary":null,
		 "barriers":[{"reason":"deadline","unfinished":[
		   {"topic":"01","target":"7","received":"7","processed":"6",
		    "unresolvedWelcomes":[],"inactive":false,"cause":null}]}]}
		"""
		let details = try XCTUnwrap(FfiError.Error(message: message).streamFailureDetails)
		XCTAssertEqual(details.kind, .barrier)
		XCTAssertEqual(details.code, "BarrierError::Incomplete")
		XCTAssertEqual(details.barriers.first?.reason, .deadline)
		XCTAssertEqual(details.barriers.first?.unfinished.first?.target, 7)
		XCTAssertEqual(details.barriers.first?.unfinished.first?.processed, 6)
		XCTAssertNil(FfiError.Error(message: "ordinary error").streamFailureDetails)
		struct OrdinaryError: Error {}
		XCTAssertNil(OrdinaryError().streamFailureDetails)
	}
}
