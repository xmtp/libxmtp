//
//  AuthenticationTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

import secp256k1
import XCTest
@testable import XMTP

final class AuthenticationTests: XCTestCase {
	func testCreateToken() async throws {
		let key = try PrivateKey.generate()
		let identity = try PrivateKey.generate()

		// Prompt them to sign "XMTP : Create Identity ..."
		let authorized = try await key.createIdentity(identity)

		// Create the `Authorization: Bearer $authToken` for API calls.
		let authToken = try await authorized.createAuthToken()

		guard let tokenData = authToken.data(using: .utf8),
		      let base64TokenData = Data(base64Encoded: tokenData)
		else {
			XCTFail("could not get token data")
			return
		}

		let token = try Token(serializedData: base64TokenData)
		let authData = try AuthData(serializedData: token.authDataBytes)

		XCTAssertEqual(authData.walletAddr, authorized.address)
	}

	func testEnablingSavingAndLoadingOfStoredKeys() async throws {
		let alice = try PrivateKey.generate()
		let identity = try PrivateKey.generate()

		let authorized = try await alice.createIdentity(identity)

		let bundle = try authorized.toBundle
		let encryptedBundle = try await bundle.encrypted(with: alice)

		let decrypted = try await encryptedBundle.decrypted(with: alice)
		XCTAssertEqual(decrypted.v1.identityKey.secp256K1.bytes, identity.secp256K1.bytes)
		XCTAssertEqual(decrypted.v1.identityKey.publicKey, authorized.authorized)
	}
}
