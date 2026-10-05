import XCTest
import XmtpSdk

final class AttachmentEventDiagnosticsTests: XCTestCase {
	func testDirectAndNestedDiagnosticsHideSignedURLAndKeepFields() {
		let url = "https://example.invalid/file?token=event-attachment-access-token"
		let attachment = AttachmentRef(attachmentKey: "attachment", url: url, contentDigest: "digest")
		let failed = AttachmentFailed(
			attachmentKey: attachment.attachmentKey, url: url,
			contentDigest: attachment.contentDigest, cause: "local_storage",
		)
		let events: [ClientEvent] = [
			.attachmentUploadStarted(attachmentUploadStarted: attachment),
			.attachmentUploadCompleted(attachmentUploadCompleted: attachment),
			.attachmentUploadFailed(attachmentUploadFailed: failed),
			.attachmentDownloadStarted(attachmentDownloadStarted: attachment),
			.attachmentDownloadCompleted(attachmentDownloadCompleted: attachment),
			.attachmentDownloadFailed(attachmentDownloadFailed: failed),
			.attachmentDeleted(attachmentDeleted: attachment),
		]
		var forms = [
			String(describing: attachment), String(reflecting: attachment), "\(attachment)",
			String(describing: failed), String(reflecting: failed), "\(failed)",
			String(describing: [attachment]), String(reflecting: ["failed": failed]),
		]
		forms += events.flatMap { [String(describing: $0), String(reflecting: $0)] }
		for value in forms {
			XCTAssertFalse(value.contains("event-attachment-access-token"), "Attachment event diagnostic exposed its URL")
		}
		XCTAssertEqual(attachment.url, url)
		XCTAssertEqual(failed.url, url)
		XCTAssertEqual(attachment.attachmentKey, "attachment")
		XCTAssertEqual(failed.contentDigest, "digest")
		XCTAssertEqual(failed.cause, "local_storage")
	}
}
