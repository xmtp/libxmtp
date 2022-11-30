//
//  PublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import XMTPProto

typealias PublicKey = Xmtp_MessageContents_PublicKey

enum PublicKeyError: Error {
	case noSignature
}

extension PublicKey {
	init(_ signedPublicKey: SignedPublicKey) throws {
		self.init()

		let unsignedPublicKey = try UnsignedPublicKey(serializedData: signedPublicKey.keyBytes)

		timestamp = unsignedPublicKey.createdNs
		secp256K1Uncompressed.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
		signature = signedPublicKey.signature
	}

	init(_ data: Data) throws {
		self.init()

		timestamp = UInt64(Date().millisecondsSinceEpoch)
		secp256K1Uncompressed.bytes = data
	}

	func recoverWalletSignerPublicKey() throws -> PublicKey {
		if !hasSignature {
			throw PublicKeyError.noSignature
		}

		var slimKey = PublicKey()
		slimKey.timestamp = timestamp
		slimKey.secp256K1Uncompressed.bytes = secp256K1Uncompressed.bytes

		let sigText = Signature.createIdentityText(key: try slimKey.serializedData())
		let sigHash = try Signature.ethHash(sigText)

		let pubKeyData = try KeyUtil.recoverPublicKey(message: sigHash, signature: signature.rawData)
		return try PublicKey(pubKeyData)
	}

	var walletAddress: String {
		KeyUtil.generateAddress(from: secp256K1Uncompressed.bytes).toChecksumAddress()
	}
}
