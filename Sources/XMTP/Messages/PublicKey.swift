//
//  PublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import secp256k1
import XMTPProto

typealias PublicKey = Xmtp_MessageContents_PublicKey

extension PublicKey {
	init(_ signedPublicKey: SignedPublicKey) throws {
		self.init()

		let unsignedPublicKey = try UnsignedPublicKey(serializedData: signedPublicKey.keyBytes)

		timestamp = unsignedPublicKey.createdNs
		secp256K1Uncompressed.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
	}
}
