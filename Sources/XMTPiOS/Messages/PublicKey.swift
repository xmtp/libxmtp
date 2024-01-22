//
//  PublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation

import LibXMTP
import web3
import CryptoKit

typealias PublicKey = Xmtp_MessageContents_PublicKey

enum PublicKeyError: String, Error {
	case noSignature, invalidPreKey, addressNotFound, invalidKeyString
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

	init(_ string: String) throws {
		self.init()

		guard let bytes = string.web3.bytesFromHex else {
			throw PublicKeyError.invalidKeyString
		}

		try self.init(Data(bytes))
	}

	func recoverWalletSignerPublicKey() throws -> PublicKey {
		if !hasSignature {
			throw PublicKeyError.noSignature
		}

		var slimKey = PublicKey()
		slimKey.timestamp = timestamp
		slimKey.secp256K1Uncompressed.bytes = secp256K1Uncompressed.bytes

		let sigText = Signature.createIdentityText(key: try slimKey.serializedData())
		let message = try Signature.ethPersonalMessage(sigText)

		let pubKeyData = try KeyUtilx.recoverPublicKeyKeccak256(from: signature.rawData, message: message)
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

		let pubKeyData = try KeyUtilx.recoverPublicKeySHA256(from: signature.rawData, message: bytesToSign)
		return try PublicKey(pubKeyData)
	}

	var walletAddress: String {
		KeyUtilx.generateAddress(from: secp256K1Uncompressed.bytes).toChecksumAddress()
	}
}
