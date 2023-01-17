//
//  SignedPublicKeyBundle.swift
//
//
//  Created by Pat Nakajima on 11/23/22.
//

import XMTPProto

typealias SignedPublicKeyBundle = Xmtp_MessageContents_SignedPublicKeyBundle

extension SignedPublicKeyBundle {
	init(_ publicKeyBundle: PublicKeyBundle) throws {
		self.init()

		identityKey = SignedPublicKey.fromLegacy(publicKeyBundle.identityKey)
		identityKey.signature = publicKeyBundle.identityKey.signature
		preKey = SignedPublicKey.fromLegacy(publicKeyBundle.preKey)
		preKey.signature = publicKeyBundle.preKey.signature
	}

	func equals(_ other: SignedPublicKeyBundle) -> Bool {
		return identityKey == other.identityKey && preKey == other.preKey
	}

	var walletAddress: String {
		get throws {
			return try identityKey.recoverWalletSignerPublicKey().walletAddress
		}
	}
}
