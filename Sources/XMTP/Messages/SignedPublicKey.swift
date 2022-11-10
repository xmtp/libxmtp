//
//  SignedPublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import XMTPProto

typealias SignedPublicKey = Xmtp_MessageContents_SignedPublicKey

extension SignedPublicKey {
	init(_ publicKey: PublicKey, signature: Signature) throws {
		self.init()
		self.signature = signature
		keyBytes = try publicKey.serializedData()
	}
}
