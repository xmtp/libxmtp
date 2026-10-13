import Foundation
import XCTest
import XmtpSdk

final class StorageLifecycleTests: XCTestCase {
	// verifies: STORE-007, STORE-009, STORE-010, STORE-017, STORE-018
	func testFileStorageKeepsHistoryAndClosesBeforeDeletion() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let path = root.appendingPathComponent("client.db3").path
		let options = liveOptions(storage: StorageOptions(location: .explicit(
			dbPath: path, attachmentsDir: root.appendingPathComponent("attachments").path,
		)))
		let signer = await generateLocalSigner()
		do {
			_ = try await SDKClient.build(identity: signer.identity(), options: options)
			XCTFail("A build opened a new identity")
		} catch XmtpError.IdentityNotFound {}
		XCTAssertFalse(FileManager.default.fileExists(atPath: path))
		try await withClients { scope in
			let client = try await scope.create(signer: signer, options: options)
			let storedPath = try await client.storage().path()
			XCTAssertEqual(storedPath, path)
			let inbox = client.inboxId()
			let group = try await client.conversations.createGroup(members: [InboxId]())
			let id = try await group.sendText(text: "stored before reconnect")
			try await client.storage().reconnect()
			let reopenedPath = try await client.storage().path()
			XCTAssertEqual(reopenedPath, path)
			XCTAssertEqual(client.inboxId(), inbox)
			let message = try await client.conversations.getMessageById(id: id)
			XCTAssertEqual(message?.id, id)
			try await client.storage().delete()
			XCTAssertFalse(FileManager.default.fileExists(atPath: path))
			do {
				_ = try await client.conversations.listGroups(options: nil)
				XCTFail("Storage deletion left the client open")
			} catch XmtpError.ClientClosed {}
		}
	}

	// verifies: STORE-009, STORE-011
	func testInMemoryDeletionFailsWithoutClosingClient() async throws {
		try await withClients { scope in
			let client = try await scope.create(
				signer: generateLocalSigner(),
				options: liveOptions(storage: StorageOptions(location: .inMemory)),
			)
			let path = try await client.storage().path()
			XCTAssertNil(path)
			do {
				try await client.storage().delete()
				XCTFail("In-memory storage deletion succeeded")
			} catch let XmtpError.InvalidInput(details) {
				XCTAssertEqual(details.code, "InvalidInput")
				XCTAssertFalse(details.retryable)
			}
			let registered = try await client.isRegistered()
			XCTAssertTrue(registered)
		}
	}

	// verifies: FORK-070
	func testForkOptionsCrossNativeBoundary() async throws {
		for policy in [ForkRecoveryPolicy.none, .allowlistedGroups, .all] {
			for disabled in [false, true] {
				var options = liveOptions(storage: StorageOptions(location: .inMemory))
				options.registration = RegistrationOptions(auto: false)
				options.forkRecovery = ForkRecoveryOptions(
					policy: policy,
					groups: [String(repeating: "13", count: 16)],
					disableResponses: disabled,
				)
				try await withClients { scope in
					let client = try await scope.create(signer: generateLocalSigner(), options: options)
					XCTAssertEqual(client.options().forkRecovery, options.forkRecovery)
				}
			}
		}
	}
}
