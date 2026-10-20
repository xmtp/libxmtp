import Foundation
import XCTest
import XmtpSdk

final class LocalSigningKeyTests: XCTestCase {
	/// The generated `extension LocalSigningKey: Signer` lets client creation take
	/// the key itself. Rust `signer/local_tests.rs` tests export and restore.
	func testKeyIsAcceptedDirectlyAsASigner() async throws {
		let key = await SDKClient.generateKeys()
		let identity = try await key.identity()
		try await withClients { scope in
			let client = try await scope.create(signer: key)
			let registered = try await client.isRegistered()
			XCTAssertTrue(registered)
			XCTAssertEqual(client.identity(), identity)
		}
	}

	func testKnownKeyAndInvalidImports() async throws {
		let fixed = "0x" + String(repeating: "0", count: 63) + "1"
		let key = try await LocalSigningKey.fromHex(key: fixed)
		let identity = try await key.identity()
		XCTAssertEqual(identity.identifier, "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
		let exported = key.toHex()
		XCTAssertEqual(exported, fixed)
		XCTAssertFalse(String(describing: key).contains(String(fixed.dropFirst(2))))
		for bytes in [Data(repeating: 0, count: 31), Data(repeating: 0, count: 32), Data(repeating: 255, count: 32)] {
			do {
				_ = try await LocalSigningKey.fromBytes(key: bytes)
				XCTFail("accepted invalid private key")
			} catch XmtpError.InvalidInput {}
		}
	}
}
