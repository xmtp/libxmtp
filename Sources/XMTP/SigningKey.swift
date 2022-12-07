//
//  SigningKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1

// Anything that can sign should be a SigningKey (like a private key or a wallet).
public protocol SigningKey {
	var address: String { get }
	func sign(_ data: Data) async throws -> Signature
	func sign(message: String) async throws -> Signature
}

extension SigningKey {
	func createIdentity(_ identity: PrivateKey) async throws -> AuthorizedIdentity {
		var slimKey = PublicKey()
		slimKey.timestamp = UInt64(Date().millisecondsSinceEpoch)
		slimKey.secp256K1Uncompressed = identity.publicKey.secp256K1Uncompressed

		let signatureText = Signature.createIdentityText(key: try slimKey.serializedData())
		let signature = try await sign(message: signatureText)

		let digest = try Signature.ethHash(signatureText)
		let recoveredKey = try KeyUtil.recoverPublicKey(message: digest, signature: signature.rawData)
		let address = KeyUtil.generateAddress(from: recoveredKey).toChecksumAddress()

		var authorized = PublicKey()
		authorized.secp256K1Uncompressed = slimKey.secp256K1Uncompressed
		authorized.timestamp = slimKey.timestamp
		authorized.signature = signature

		return AuthorizedIdentity(address: address, authorized: authorized, identity: identity)
	}
}
