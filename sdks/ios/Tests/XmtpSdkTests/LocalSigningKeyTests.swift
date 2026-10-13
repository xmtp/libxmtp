import Foundation
import XCTest
import XmtpSdk

final class LocalSigningKeyTests: XCTestCase {
	func testExportRestoreAndSignerConformance() async throws {
		let key = await SDKClient.generateKeys()
		let bytes = await key.toBytes()
		let hex = await key.toHex()
		XCTAssertEqual(bytes.count, 32)
		let restored = try await LocalSigningKey.fromHex(key: hex)
		let restoredBytes = await restored.toBytes()
		XCTAssertEqual(restoredBytes, bytes)
		let owner: Signer = try await LocalSigningKey.fromBytes(key: bytes)
		let identity = try await owner.identity()
		let originalIdentity = try await key.identity()
		XCTAssertEqual(identity, originalIdentity)
		try await withClients { scope in
			let client = try await scope.create(signer: restored)
			let registered = try await client.isRegistered()
			XCTAssertTrue(registered)
		}
	}

	func testKnownKeyAndInvalidImports() async throws {
		let fixed = "0x" + String(repeating: "0", count: 63) + "1"
		let key = try await LocalSigningKey.fromHex(key: fixed)
		let identity = try await key.identity()
		XCTAssertEqual(identity.identifier, "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
		let exported = await key.toHex()
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
