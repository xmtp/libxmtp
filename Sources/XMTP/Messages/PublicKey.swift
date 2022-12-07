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

enum PublicKeyError: String, Error {
	case noSignature, invalidPreKey, addressNotFound
}

extension PublicKey {
	init(_ signedPublicKey: SignedPublicKey) throws {
		self.init()

		let unsignedPublicKey = try PublicKey(serializedData: signedPublicKey.keyBytes)

		timestamp = unsignedPublicKey.timestamp
		secp256K1Uncompressed.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
		var signature = signedPublicKey.signature

		if !signature.walletEcdsaCompact.bytes.isEmpty {
			signature.ecdsaCompact.bytes = signedPublicKey.signature.walletEcdsaCompact.bytes
			signature.ecdsaCompact.recovery = signedPublicKey.signature.walletEcdsaCompact.recovery
		}

		self.signature = signature
	}

	init(_ unsignedPublicKey: UnsignedPublicKey) {
		self.init()
		secp256K1Uncompressed.bytes = unsignedPublicKey.secp256K1Uncompressed.bytes
		timestamp = unsignedPublicKey.createdNs / 1_000_000
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

	func recoverKeySignedPublicKey() throws -> PublicKey {
		if !hasSignature {
			throw PublicKeyError.noSignature
		}

		// We don't want to include the signature in the key bytes
		var slimKey = PublicKey()
		slimKey.secp256K1Uncompressed.bytes = secp256K1Uncompressed.bytes
		slimKey.timestamp = timestamp
		let bytesToSign = try slimKey.serializedData()

		let pubKeyData = try KeyUtil.recoverPublicKey(message: Data(SHA256.hash(data: bytesToSign)), signature: signature.rawData)
		return try PublicKey(pubKeyData)
	}

	var walletAddress: String {
		KeyUtil.generateAddress(from: secp256K1Uncompressed.bytes).toChecksumAddress()
	}
}
