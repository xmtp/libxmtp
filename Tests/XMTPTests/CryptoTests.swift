//
//  CryptoTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import secp256k1
import XCTest
@testable import XMTP

final class CryptoTests: XCTestCase {
	func testCodec() throws {
		let message = Data([5, 5, 5])
		let secret = Data([1, 2, 3, 4])
		let encrypted = try Crypto.encrypt(secret, message)
		let decrypted = try Crypto.decrypt(secret, encrypted)
		XCTAssertEqual(message, decrypted)
	}

	func testDecryptingKnownCypherText() throws {
		let message = Data([5, 5, 5])
		let secret = Data([1, 2, 3, 4])
		let encrypted = try CipherText(serializedData: Data([
			// This was generated using xmtp-js code for encrypt().
			10, 69, 10, 32, 23, 10, 217, 190, 235, 216, 145,
			38, 49, 224, 165, 169, 22, 55, 152, 150, 176, 65,
			207, 91, 45, 45, 16, 171, 146, 125, 143, 60, 152,
			128, 0, 120, 18, 12, 219, 247, 207, 184, 141, 179,
			171, 100, 251, 171, 120, 137, 26, 19, 216, 215, 152,
			167, 118, 59, 93, 177, 53, 242, 147, 10, 87, 143,
			27, 245, 154, 169, 109,
		]))

		let decrypted = try Crypto.decrypt(secret, encrypted)
		XCTAssertEqual(message, decrypted)
	}
}
