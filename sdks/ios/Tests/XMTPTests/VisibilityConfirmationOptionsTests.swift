import XCTest

@testable import XMTPiOS

final class VisibilityConfirmationOptionsTests: XCTestCase {
	func testToFfiMapsAllFields() {
		let options = VisibilityConfirmationOptions(
			quorumPercentage: 0.75,
			quorumAbsolute: 3,
			timeoutMs: 10000
		)
		let ffi = options.toFfi()
		XCTAssertEqual(ffi.quorumPercentage, 0.75)
		XCTAssertEqual(ffi.quorumAbsolute, 3)
		XCTAssertEqual(ffi.timeoutMs, 10000)
	}

	func testToFfiDefaultsToAllNil() {
		let options = VisibilityConfirmationOptions()
		let ffi = options.toFfi()
		XCTAssertNil(ffi.quorumPercentage)
		XCTAssertNil(ffi.quorumAbsolute)
		XCTAssertNil(ffi.timeoutMs)
	}
}
