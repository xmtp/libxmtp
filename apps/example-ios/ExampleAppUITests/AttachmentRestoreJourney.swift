import ExampleCore
import ExampleSDK
import Foundation
import UIKit
import XCTest
import XmtpSdk

@MainActor final class AttachmentRestoreJourney: XCTestCase {
	func testExtensionlessImageRestoresItsVerifiedMetadata() async throws {
		let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		let backend = ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:5050"
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: ClientOptions(
			backend: .options(options: BackendOptions(url: backend)),
			storage: StorageOptions(location: .explicit(dbPath: directory.appendingPathComponent("client.db").path,
			                                            attachmentsDir: directory.appendingPathComponent("attachments").path)),
			deviceSync: false,
		))
		addTeardownBlock {
			try await client.end()
			try FileManager.default.removeItem(at: directory)
		}
		let bytes = UIGraphicsImageRenderer(size: CGSize(width: 2, height: 2)).pngData { context in
			UIColor.systemTeal.setFill(); context.fill(CGRect(x: 0, y: 0, width: 2, height: 2))
		}
		let pending = try await client.attachments().create(source: .bytes(
			bytes: bytes,
			filename: "photo",
			mimeType: "image/png",
		))
		let remote = pending.remoteAttachment()
		let descriptor = FileDescriptor(url: remote.url, digest: remote.contentDigest, secret: remote.secret,
		                                salt: remote.salt, nonce: remote.nonce, scheme: remote.scheme,
		                                length: remote.contentLength, filename: remote.filename)
		let firstValue = try await SDKAttachmentGateway(client: client).localFile(descriptor)
		let first = try XCTUnwrap(firstValue)
		let reopenedValue = try await SDKAttachmentGateway(client: client).localFile(descriptor)
		let reopened = try XCTUnwrap(reopenedValue)
		XCTAssertEqual(first.mimeType, "image/png")
		XCTAssertEqual(reopened.mimeType, "image/png")
		XCTAssertEqual(reopened.filename, "photo")
		XCTAssertEqual(try Data(contentsOf: reopened.url), bytes)
	}
}
