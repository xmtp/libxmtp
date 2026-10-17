import Foundation
import XCTest
import XmtpSdk

final class SecretRecordDiagnosticsTests: XCTestCase {
	func testEncodedContentDiagnosticsHideSecretParameter() {
		let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
		let encoded = EncodedContent(
			type: ContentTypeId(authorityId: "xmtp.org", typeId: "remoteAttachment", versionMajor: 1, versionMinor: 0),
			parameters: ["secret": secret, "filename": "file"],
			fallback: "file",
			content: Data([1]),
		)
		let forms = [
			String(describing: encoded),
			String(reflecting: encoded),
			"\(encoded)",
			String(reflecting: [encoded]),
			String(reflecting: ["content": encoded]),
		]
		for form in forms {
			XCTAssertFalse(form.contains(secret), "EncodedContent diagnostics exposed the decryption secret")
		}
		XCTAssertEqual(encoded.parameters["secret"], secret)
		XCTAssertEqual(encoded.parameters["filename"], "file")
		XCTAssertEqual(encoded.content, Data([1]))
	}
}
