//
//  SignedPrivateKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import XMTPProto

public typealias SignedPrivateKey = Xmtp_MessageContents_SignedPrivateKey

extension SignedPrivateKey {
	static func fromLegacy(_ key: PrivateKey, signedByWallet: Bool? = false) -> SignedPrivateKey {
		var signedPrivateKey = SignedPrivateKey()

		signedPrivateKey.createdNs = key.timestamp * 1_000_000
		signedPrivateKey.secp256K1.bytes = key.secp256K1.bytes
		signedPrivateKey.publicKey = SignedPublicKey.fromLegacy(key.publicKey, signedByWallet: signedByWallet)
		signedPrivateKey.publicKey.signature = key.publicKey.signature

		return signedPrivateKey
	}

	func sign(_ data: Data) async throws -> Signature {
		let key = try PrivateKey(secp256K1.bytes)
		return try await key.sign(data)
	}

	func matches(_ signedPublicKey: SignedPublicKey) -> Bool {
		do {
			return try publicKey.recoverKeySignedPublicKey().walletAddress ==
				(try signedPublicKey.recoverKeySignedPublicKey().walletAddress)
		} catch {
			return false
		}
	}
}
