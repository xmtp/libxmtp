//
//  SignedPrivateKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import XMTPProto

typealias SignedPrivateKey = Xmtp_MessageContents_SignedPrivateKey

extension SignedPrivateKey {
	static func fromLegacy(_ key: PrivateKey, signedByWallet: Bool? = false) throws -> SignedPrivateKey {
		var signedPrivateKey = SignedPrivateKey()

		signedPrivateKey.createdNs = key.timestamp * 1_000_000
		signedPrivateKey.secp256K1.bytes = key.secp256K1.bytes
		signedPrivateKey.publicKey = try SignedPublicKey.fromLegacy(key.publicKey, signedByWallet: signedByWallet)
		signedPrivateKey.publicKey.signature = key.publicKey.signature

		return signedPrivateKey
	}

	func matches(_ signedPublicKey: SignedPublicKey) -> Bool {
		return publicKey.keyBytes == signedPublicKey.keyBytes
	}
}
