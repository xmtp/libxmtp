import Foundation
import SQLite3
import XCTest
import XmtpSdk

final class PublicMessagePrivacyTests: XCTestCase {
	func testKnownMessageIdDoesNotExposeInternalSyncGroup() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
		defer { try? FileManager.default.removeItem(at: root) }
		let database = root.appendingPathComponent("messages.sqlite")
		let options = ClientOptions(
			backend: .options(options: BackendOptions(url: ProcessInfo.processInfo
					.environment["XMTP_BACKEND_URL"] ?? "http://127.0.0.1:6250")),
			storage: StorageOptions(location: .explicit(
				dbPath: database.path,
				attachmentsDir: root.appendingPathComponent("attachments").path,
			)),
			deviceSync: false,
		)
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let id = try await group.sendText(text: "internal-sync-payload-canary")
		let groupId = group.id()
		let identity = client.identity()
		try await client.end()

		// Mark the stored fixture as an internal sync group after closing the SDK.
		// ConversationType::Sync has database value 3.
		var connection: OpaquePointer?
		XCTAssertEqual(sqlite3_open(database.path, &connection), SQLITE_OK)
		guard let connection else { return XCTFail("Fixture database did not open") }
		let result = sqlite3_exec(
			connection,
			"UPDATE groups SET conversation_type = 3 WHERE id = X'\(groupId)'",
			nil,
			nil,
			nil,
		)
		let changed = sqlite3_changes(connection)
		sqlite3_close(connection)
		XCTAssertEqual(result, SQLITE_OK)
		XCTAssertEqual(changed, 1)

		let reopened = try await SDKClient.build(identity: identity, options: options)
		let found = try await reopened.conversations().getMessageById(id: id)
		try await reopened.end()
		XCTAssertNil(found, "Public lookup exposed the internal sync group message")
	}
}
