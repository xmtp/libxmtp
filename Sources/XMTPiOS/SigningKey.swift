//
//  SigningKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import web3
import LibXMTP

/// Defines a type that is used by a ``Client`` to sign keys and messages.
///
/// You can use ``Account`` for an easier WalletConnect flow, or ``PrivateKey``
/// for quick key generation.
///
/// > Tip: You can make your own object that conforms to ``SigningKey`` if you want to
/// handle key management yourself.
public protocol SigningKey {
	/// A wallet address for this key
	var address: String { get }

	/// Sign the data and return a secp256k1 compact recoverable signature.
	func sign(_ data: Data) async throws -> Signature

	/// Pass a personal Ethereum signed message string text to be signed, returning
	/// a secp256k1 compact recoverable signature. You can use ``Signature.ethPersonalMessage`` to generate this text.
	func sign(message: String) async throws -> Signature
}

extension SigningKey {
	func createIdentity(_ identity: PrivateKey, preCreateIdentityCallback: PreEventCallback? = nil) async throws -> AuthorizedIdentity {
		var slimKey = PublicKey()
		slimKey.timestamp = UInt64(Date().millisecondsSinceEpoch)
		slimKey.secp256K1Uncompressed = identity.publicKey.secp256K1Uncompressed

		try await preCreateIdentityCallback?()

		let signatureText = Signature.createIdentityText(key: try slimKey.serializedData())
		let signature = try await sign(message: signatureText)

		let message = try Signature.ethPersonalMessage(signatureText)
		let recoveredKey = try KeyUtilx.recoverPublicKeyKeccak256(from: signature.rawData, message: message)
		let address = KeyUtilx.generateAddress(from: recoveredKey).toChecksumAddress()

		var authorized = PublicKey()
		authorized.secp256K1Uncompressed = slimKey.secp256K1Uncompressed
		authorized.timestamp = slimKey.timestamp
		authorized.signature = signature

		return AuthorizedIdentity(address: address, authorized: authorized, identity: identity)
	}
}
