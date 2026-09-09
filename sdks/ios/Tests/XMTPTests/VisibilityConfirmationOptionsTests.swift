import XCTest
@testable import XMTPiOS

final class VisibilityConfirmationOptionsTests: XCTestCase {
	func testToFfiMapsAllFields() {
		let options = VisibilityConfirmationOptions(
			timeoutMs: 10000
		)
		let ffi = options.toFfi()
		XCTAssertEqual(ffi.timeoutMs, 10000)
	}

	func testToFfiDefaultsToAllNil() {
		let options = VisibilityConfirmationOptions()
		let ffi = options.toFfi()
		XCTAssertNil(ffi.timeoutMs)
	}
}
