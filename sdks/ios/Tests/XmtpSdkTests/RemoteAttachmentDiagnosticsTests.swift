import XCTest
import XmtpSdk

final class RemoteAttachmentDiagnosticsTests: XCTestCase {
	func testDirectAndNestedDiagnosticsHideSignedURLAndKeepFields() {
		let url = "https://example.invalid/attachment?token=attachment-access-token"
		let secret = Data([17, 61, 33])
		let attachment = RemoteAttachment(
			url: url, contentDigest: "digest", secret: secret,
			salt: Data([2]), nonce: Data([3]), scheme: "https",
			contentLength: 64, filename: "attachment.txt",
		)
		let multi = MultiRemoteAttachment(attachments: [attachment])
		let forms = [
			String(describing: attachment),
			String(reflecting: attachment),
			"\(attachment)",
			String(describing: [attachment]),
			String(reflecting: ["attachment": attachment]),
			String(describing: multi),
			String(reflecting: multi),
		]
		for value in forms {
			XCTAssertFalse(value.contains("attachment-access-token"), "Attachment diagnostic exposed its URL")
		}
		XCTAssertEqual(attachment.url, url)
		XCTAssertEqual(attachment.secret, secret)
		XCTAssertEqual(attachment.contentDigest, "digest")
		XCTAssertEqual(attachment.contentLength, 64)
		XCTAssertEqual(attachment.filename, "attachment.txt")
	}
}
