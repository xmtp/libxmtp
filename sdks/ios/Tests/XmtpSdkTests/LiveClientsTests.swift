import Foundation
import XCTest
import XmtpSdk

/// The `withClients` and `withLiveClients` helpers in `LiveBackend.swift`.
final class LiveClientsTests: XCTestCase {
	/// A body that throws still ends every client, and its error reaches the caller.
	func testClientsEndWhenTheBodyThrows() async throws {
		let created = Shared<[SDKClient]>([])
		do {
			try await withLiveClients(2) { clients in
				created.update { $0 = clients }
				throw TestFailure("body failed")
			}
			XCTFail("The helper did not throw the body error")
		} catch let error as TestFailure {
			XCTAssertEqual(error.description, "body failed")
		}
		try await assertEnded(created.value, count: 2)
	}

	/// A body that returns gives its value, and every client ends.
	func testClientsEndWhenTheBodyReturns() async throws {
		let created = Shared<[SDKClient]>([])
		let value = try await withLiveClients(2) { clients in
			created.update { $0 = clients }
			return clients.count
		}
		XCTAssertEqual(value, 2)
		try await assertEnded(created.value, count: 2)
	}

	/// A create that fails after an earlier one still ends the earlier client,
	/// and the create error reaches the caller.
	func testClientsEndWhenALaterCreateFails() async throws {
		let created = Shared<[SDKClient]>([])
		let unknown = try await generateLocalSigner().identity()
		do {
			try await withClients { scope in
				let first = try await scope.create(signer: generateLocalSigner())
				created.update { $0 = [first] }
				_ = try await scope.build(identity: unknown, options: liveOptions())
				XCTFail("A build with an unknown identity opened a client")
			}
			XCTFail("The helper did not throw the create error")
		} catch XmtpError.IdentityNotFound {}
		try await assertEnded(created.value, count: 1)
	}

	/// A client that the body ended itself ends again without an error, so a
	/// test can end a client mid-body inside the helper.
	func testClientEndedByTheBodyEndsAgainWithoutError() async throws {
		let created = Shared<[SDKClient]>([])
		let value = try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner())
			created.update { $0 = [client] }
			try await client.end()
			return 1
		}
		XCTAssertEqual(value, 1)
		try await assertEnded(created.value, count: 1)
	}

	private func assertEnded(_ clients: [SDKClient], count: Int) async throws {
		XCTAssertEqual(clients.count, count)
		for client in clients {
			do {
				try await client.conversations.sync()
				XCTFail("A client stayed open after the helper returned")
			} catch XmtpError.ClientClosed {}
		}
	}
}
