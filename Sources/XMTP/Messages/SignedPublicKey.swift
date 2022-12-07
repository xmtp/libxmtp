//
//  SignedPublicKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import CryptoKit
import Foundation
import secp256k1
import XMTPProto

typealias SignedPublicKey = Xmtp_MessageContents_SignedPublicKey

extension SignedPublicKey {
	static func fromLegacy(_ legacyKey: PublicKey, signedByWallet: Bool? = false) throws -> SignedPublicKey {
		var signedPublicKey = SignedPublicKey()

		var publicKey = PublicKey()
		publicKey.secp256K1Uncompressed = legacyKey.secp256K1Uncompressed
		publicKey.timestamp = legacyKey.timestamp

		signedPublicKey.keyBytes = try publicKey.serializedData()
		signedPublicKey.signature = legacyKey.signature

		if let signedByWallet, signedByWallet {
			var signature = legacyKey.signature
			signature.walletEcdsaCompact.bytes = legacyKey.signature.ecdsaCompact.bytes
			signature.walletEcdsaCompact.recovery = legacyKey.signature.ecdsaCompact.recovery
			signedPublicKey.signature = signature
		}

		return signedPublicKey
	}

	init(_ publicKey: PublicKey, signature: Signature) throws {
		self.init()
		self.signature = signature

		var unsignedKey = PublicKey()
		unsignedKey.timestamp = publicKey.timestamp
		unsignedKey.secp256K1Uncompressed.bytes = publicKey.secp256K1Uncompressed.bytes

		keyBytes = try unsignedKey.serializedData()
	}

	var secp256K1Uncompressed: PublicKey.Secp256k1Uncompressed {
		// swiftlint:disable force_try
		let key = try! PublicKey(serializedData: keyBytes)
		// swiftlint:enable force_try
		return key.secp256K1Uncompressed
	}

	func verify(key: SignedPublicKey) throws -> Bool {
		if !key.hasSignature {
			return false
		}

		return try signature.verify(signedBy: try PublicKey(key), digest: key.keyBytes)
	}

	func recoverKeySignedPublicKey() throws -> PublicKey {
		let publicKey = try PublicKey(self)

		// We don't want to include the signature in the key bytes
		var slimKey = PublicKey()
		slimKey.secp256K1Uncompressed.bytes = secp256K1Uncompressed.bytes
		slimKey.timestamp = publicKey.timestamp
		let bytesToSign = try slimKey.serializedData()

		let pubKeyData = try KeyUtil.recoverPublicKey(message: Data(SHA256.hash(data: bytesToSign)), signature: publicKey.signature.rawData)
		return try PublicKey(pubKeyData)
	}

	func recoverWalletSignerPublicKey() throws -> PublicKey {
		let sigText = Signature.createIdentityText(key: keyBytes)
		let sigHash = try Signature.ethHash(sigText)

		let pubKeyData = try KeyUtil.recoverPublicKey(message: sigHash, signature: signature.rawData)

		return try PublicKey(pubKeyData)
	}
}
