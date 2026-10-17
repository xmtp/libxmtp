import XCTest
import XmtpSdk

final class EventFilterDefaultsTests: XCTestCase {
	func testOptionalFilterFieldsCanBeOmitted() {
		let filter = EventFilter(kinds: [.messageReceived])

		XCTAssertEqual(filter.kinds, [.messageReceived])
		XCTAssertNil(filter.groupIds)
		XCTAssertNil(filter.contentTypes)
		XCTAssertFalse(filter.referencesOwnMessages)
	}
}
