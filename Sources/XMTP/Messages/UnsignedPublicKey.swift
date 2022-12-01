//
//  UnsignedPublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import XMTPProto

typealias UnsignedPublicKey = Xmtp_MessageContents_UnsignedPublicKey

extension UnsignedPublicKey {
	static func generate() throws -> UnsignedPublicKey {
		var unsigned = UnsignedPublicKey()
		let key = try PrivateKey.generate()
		let createdNs = Date().millisecondsSinceEpoch
		unsigned.secp256K1Uncompressed.bytes = key.publicKey.secp256K1Uncompressed.bytes
		unsigned.createdNs = UInt64(createdNs)
		return unsigned
	}

	init(_ publicKey: PublicKey) {
		self.init()

		createdNs = publicKey.timestamp
		secp256K1Uncompressed.bytes = publicKey.secp256K1Uncompressed.bytes
	}
}
