import Foundation
import XCTest
import XmtpSdk

final class EventPayloadTests: XCTestCase {
	func testNamedPayloadsRoundTripAcrossNativeBuffer() throws {
		let events: [ClientEvent] = [
			.hmacKeysUpdated(hmacKeysUpdated: HmacKeysUpdated()),
			.consentChanged(consentChanged: ConsentChanged(
				entityKind: .inbox, entity: "event-payload-inbox", state: .allowed,
			)),
		]
		for event in events {
			let buffer = FfiConverterTypeClientEvent_lower(event)
			XCTAssertEqual(try FfiConverterTypeClientEvent_lift(buffer), event)
		}
	}
}
