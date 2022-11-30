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

		var unsignedKey = UnsignedPublicKey()
		unsignedKey.createdNs = publicKey.timestamp * 1_000_000
		unsignedKey.secp256K1Uncompressed.bytes = publicKey.secp256K1Uncompressed.bytes

		keyBytes = try unsignedKey.serializedData()
	}

	func recoverWalletSignerPublicKey() throws -> PublicKey {
		let sigText = Signature.createIdentityText(key: keyBytes)
		let sigHash = try Signature.ethHash(sigText)

		let pubKeyData = try KeyUtil.recoverPublicKey(message: sigHash, signature: signature.rawData)

		return try PublicKey(pubKeyData)
	}
}
