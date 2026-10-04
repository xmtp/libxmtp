import Foundation
import XCTest
import XmtpSdk

private func clientEndOptions(storage: StorageOptions = StorageOptions(location: .inMemory)) -> ClientOptions {
	ClientOptions(
		backend: .options(options: BackendOptions(
			url: ProcessInfo.processInfo.environment["XMTP_BACKEND_URL"] ?? "http://localhost:5050",
		)),
		storage: storage,
		deviceSync: false,
	)
}

final class ClientEndTests: XCTestCase {
	/// The Rust test proves native idle entry. This test checks the public result.
	func testClientEndSettlesConcurrentMessageRead() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: clientEndOptions())
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let reader = try await group.messageReader(options: nil)
		let settled = expectation(description: "The public message read settles")
		let pending = Task {
			defer { settled.fulfill() }
			do {
				return try await reader.next()
			} catch XmtpError.ClientClosed {
				return nil
			}
		}
		try await client.end()
		let result = await XCTWaiter.fulfillment(of: [settled], timeout: 2)
		XCTAssertEqual(result, .completed)
		if result != .completed {
			pending.cancel()
			try await reader.end()
		}
		let value = try await pending.value
		XCTAssertNil(value)
	}

	func testClientEndPreservesUnacknowledgedLocalDelivery() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let key = Data(repeating: 0x43, count: 32)
		let options = clientEndOptions(storage: StorageOptions(
			location: .directory(directory: root.path), encryptionKey: key,
		))
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: options)
		let group = try await client.conversations().createGroup(members: [InboxId]())
		let messageId = try await group.sendText(text: "unacknowledged before client end")
		let reader = try await group.messageReader(options: nil)
		let delivered = try await reader.next()
		XCTAssertEqual(delivered?.id, messageId)
		let identity = client.identity()
		let groupId = group.id()
		try await client.end()

		let reopened = try await SDKClient.build(identity: identity, options: options)
		let restored = try await reopened.conversations().getById(id: groupId)
		guard case let .group(reopenedGroup)? = restored else {
			try await reopened.end()
			return XCTFail("Stored group was not restored")
		}
		let replay = try await reopenedGroup.messageReader(options: nil)
		let repeated = try await replay.next()
		XCTAssertEqual(repeated?.id, messageId)
		try await replay.end()
		try await reopened.end()
	}
}
