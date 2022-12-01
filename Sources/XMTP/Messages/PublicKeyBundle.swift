//
//  PublicKeyBundle.swift
//
//
//  Created by Pat Nakajima on 11/23/22.
//

import XMTPProto

typealias PublicKeyBundle = Xmtp_MessageContents_PublicKeyBundle

extension PublicKeyBundle {
	init(_ signedPublicKeyBundle: SignedPublicKeyBundle) throws {
		self.init()
		identityKey = try PublicKey(signedPublicKeyBundle.identityKey)
		preKey = try PublicKey(signedPublicKeyBundle.preKey)
	}

	var walletAddress: String {
		get throws {
			return try identityKey.recoverWalletSignerPublicKey().walletAddress
		}
	}
}
