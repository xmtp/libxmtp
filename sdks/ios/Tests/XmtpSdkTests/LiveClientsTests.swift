import Foundation
import XCTest
import XmtpSdk

/// The `withLiveClients` helper in `LiveBackend.swift`.
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

	private func assertEnded(_ clients: [SDKClient], count: Int) async throws {
		XCTAssertEqual(clients.count, count)
		for client in clients {
			do {
				try await client.conversations().sync()
				XCTFail("A client stayed open after the helper returned")
			} catch XmtpError.ClientClosed {}
		}
	}
}
