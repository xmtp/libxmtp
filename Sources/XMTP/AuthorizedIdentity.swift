//
//  AuthorizedIdentity.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

struct AuthorizedIdentity {
	var address: String
	var authorized: PublicKey
	var identity: PrivateKey

	func createAuthToken() async throws -> String {
		let authData = AuthData(walletAddress: address)
		let authDataBytes = try authData.serializedData()
		let signature = try await identity.sign(Util.keccak256(authDataBytes))

		var token = Token()

		token.identityKey = authorized
		token.authDataBytes = authDataBytes
		token.authDataSignature = signature

		return try token.serializedData().base64EncodedString()
	}

	var toBundle: PrivateKeyBundle {
		get throws {
			var bundle = PrivateKeyBundle()
			let identity = identity
			let authorized = authorized

			bundle.v1.identityKey = identity
			bundle.v1.identityKey.publicKey = authorized
			return bundle
		}
	}
}

// In an extension so we don't lose the normal struct init()
extension AuthorizedIdentity {
	init(privateKeyBundleV1: PrivateKeyBundleV1) {
		address = privateKeyBundleV1.identityKey.walletAddress
		authorized = privateKeyBundleV1.identityKey.publicKey
		identity = privateKeyBundleV1.identityKey
	}
}
