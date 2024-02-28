//
//  CryptoTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import secp256k1
import XCTest
@testable import XMTPiOS

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

	func testMessages() async throws {
		let aliceWallet = try PrivateKey.generate()
		let bobWallet = try PrivateKey.generate()

		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)
		let bob = try await PrivateKeyBundleV1.generate(wallet: bobWallet)

		let msg = "Hello world"
		let decrypted = Data(msg.utf8)

		let alicePublic = alice.toPublicKeyBundle()
		let bobPublic = bob.toPublicKeyBundle()

		let aliceSecret = try alice.sharedSecret(peer: bobPublic, myPreKey: alicePublic.preKey, isRecipient: false)

		let encrypted = try Crypto.encrypt(aliceSecret, decrypted)

		let bobSecret = try bob.sharedSecret(peer: alicePublic, myPreKey: bobPublic.preKey, isRecipient: true)
		let bobDecrypted = try Crypto.decrypt(bobSecret, encrypted)

		let decryptedText = String(data: bobDecrypted, encoding: .utf8)

		XCTAssertEqual(decryptedText, msg)
	}
	
	func testGenerateAndValidateHmac() async throws {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let info = try Crypto.secureRandomBytes(count: 32)
		let message = try Crypto.secureRandomBytes(count: 32)
		let hmac = try Crypto.generateHmacSignature(secret: secret, info: info, message: message)
		let key = try Crypto.hkdfHmacKey(secret: secret, info: info)
		let valid = Crypto.verifyHmacSignature(key: key, signature: hmac, message: message)
		
		XCTAssertTrue(valid)
	}
	
	func testGenerateAndValidateHmacWithExportedKey() async throws {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let info = try Crypto.secureRandomBytes(count: 32)
		let message = try Crypto.secureRandomBytes(count: 32)
		let hmac = try Crypto.generateHmacSignature(secret: secret, info: info, message: message)
		let key = try Crypto.hkdfHmacKey(secret: secret, info: info)
		let exportedKey = Crypto.exportHmacKey(key: key)
		let importedKey = Crypto.importHmacKey(keyData: exportedKey)
		let valid = Crypto.verifyHmacSignature(key: importedKey, signature: hmac, message: message)
		
		XCTAssertTrue(valid)
	}
	
	func testGenerateDifferentHmacKeysWithDifferentInfos() async throws {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let info1 = try Crypto.secureRandomBytes(count: 32)
		let info2 = try Crypto.secureRandomBytes(count: 32)
		let key1 = try Crypto.hkdfHmacKey(secret: secret, info: info1)
		let key2 = try Crypto.hkdfHmacKey(secret: secret, info: info2)
		let exportedKey1 = Crypto.exportHmacKey(key: key1)
		let exportedKey2 = Crypto.exportHmacKey(key: key2)
		
		XCTAssertNotEqual(exportedKey1, exportedKey2)
	}
	
	func testValidateHmacWithWrongMessage() async throws {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let info = try Crypto.secureRandomBytes(count: 32)
		let message = try Crypto.secureRandomBytes(count: 32)
		let hmac = try Crypto.generateHmacSignature(secret: secret, info: info, message: message)
		let key = try Crypto.hkdfHmacKey(secret: secret, info: info)
		let valid = Crypto.verifyHmacSignature(
			key: key,
			signature: hmac,
			message: try Crypto.secureRandomBytes(count: 32)
		)
		
		XCTAssertFalse(valid)
	}
	
	func testValidateHmacWithWrongKey() async throws {
		let secret = try Crypto.secureRandomBytes(count: 32)
		let info = try Crypto.secureRandomBytes(count: 32)
		let message = try Crypto.secureRandomBytes(count: 32)
		let hmac = try Crypto.generateHmacSignature(secret: secret, info: info, message: message)
		let valid = Crypto.verifyHmacSignature(
			key: try Crypto.hkdfHmacKey(
				secret: try Crypto.secureRandomBytes(count: 32),
				info: try Crypto.secureRandomBytes(count: 32)),
			signature: hmac,
			message: message
		)
		
		XCTAssertFalse(valid)
	}
}
