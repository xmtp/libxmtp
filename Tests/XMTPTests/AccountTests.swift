//
//  AccountTests.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import XCTest
@testable import XMTP

class StubWalletConnection: WalletConnection {
	var key: PrivateKey
	var isConnected = false
	var wasConnectCalled = false
	var wasSignCalled = false

	init(key: PrivateKey) {
		self.key = key
	}

	func connect() async throws {
		isConnected = true
		wasConnectCalled = true
	}

	var walletAddress: String? {
		KeyUtil.generateAddress(from: key.publicKey.secp256K1Uncompressed.bytes).value
	}

	func preferredConnectionMethod() throws -> WalletConnectionMethodType {
		WalletManualConnectionMethod(redirectURI: "https://example.com").type
	}

	func sign(_ data: Data) async throws -> Data {
		wasSignCalled = true
		let sig = try await key.sign(data)

		return sig.ecdsaCompact.bytes + [UInt8(Int(sig.ecdsaCompact.recovery))]
	}
}

final class AccountTests: XCTestCase {
	func testTakesAConnectionAndConnects() async throws {
		let key = try PrivateKey.generate()
		let stubConnection = StubWalletConnection(key: key)

		let wallet = try Account(connection: stubConnection)

		try await wallet.connect()
		XCTAssert(stubConnection.wasConnectCalled, "did not call connect() on connection")

		let digest = "Hello world".web3.keccak256

		let expectedSignature = try await key.sign(digest)

		let signature = try await wallet.sign(digest)

		XCTAssertEqual(signature.ecdsaCompact.bytes, expectedSignature.ecdsaCompact.bytes)
		XCTAssertEqual(signature.ecdsaCompact.recovery, expectedSignature.ecdsaCompact.recovery)
	}
}
