import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
import XmtpSdk

struct LocalAttachmentRestoreTests {
	@Test(arguments: [false, true]) func localRestoreNeverInvokesTransfer(deletedAfterProbe: Bool) async throws {
		let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		defer { do { try FileManager.default.removeItem(at: directory) } catch { Issue.record(error) } }
		let url = directory.appendingPathComponent("photo")
		let bytes =
			try #require(
				Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aPrsAAAAASUVORK5CYII="),
			)
		try bytes.write(to: url)
		let transfer = LocalTransferProbe()
		let attachments = LocalAttachmentsFake(url: url, transfer: transfer)
		let gateway = SDKAttachmentGateway(attachments: attachments, beforeLocalRead: {
			if deletedAfterProbe { try FileManager.default.removeItem(at: url) }
		})
		let descriptor = FileDescriptor(url: "https://sender.test/file", digest: "digest", secret: Data(),
		                                salt: Data(), nonce: Data(), scheme: "https", length: 1, filename: "photo")
		let file = try await gateway.localFile(descriptor)
		#expect(await transfer.calls == 0)
		#expect((file == nil) == deletedAfterProbe)
		#expect(FileManager.default.fileExists(atPath: url.path) == !deletedAfterProbe)
		if let file {
			#expect(file.filename == "photo")
			#expect(file.mimeType == "image/png")
			#expect(try Data(contentsOf: file.url) == bytes)
		}
	}
}

private actor LocalTransferProbe {
	var calls = 0
	func record() {
		calls += 1
	}
}

private final class LocalAttachmentsFake: AttachmentsProtocol {
	let url: URL
	let transfer: LocalTransferProbe
	init(url: URL, transfer: LocalTransferProbe) {
		self.url = url; self.transfer = transfer
	}

	func localPath(remote _: RemoteAttachment) async throws -> String {
		url.path
	}

	func download(remote _: RemoteAttachment) async throws -> DownloadedAttachment {
		await transfer.record()
		try Data("downloaded again".utf8).write(to: url)
		return DownloadedAttachment(path: url.path, mimeType: "image/png", filename: "photo")
	}

	func create(source _: AttachmentSource) async throws -> PendingAttachment {
		throw ChatError.unavailable
	}

	func deleteLocal(remote _: RemoteAttachment) async throws {
		try FileManager.default.removeItem(at: url)
	}

	func listLocal() async throws -> [LocalAttachment] {
		[]
	}

	func listPending() async throws -> [PendingAttachment] {
		[]
	}

	func offered() -> Bool {
		true
	}

	func pending(remote _: RemoteAttachment) async throws -> PendingAttachment {
		throw ChatError.unavailable
	}
}
