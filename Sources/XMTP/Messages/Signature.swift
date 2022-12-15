//
//  Signature.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import XMTPProto

public typealias Signature = Xmtp_MessageContents_Signature

enum SignatureError: Error {
	case invalidMessage
}

extension Signature {
	static func ethPersonalMessage(_ message: String) throws -> Data {
		let prefix = "\u{19}Ethereum Signed Message:\n\(message.count)"

		guard var data = prefix.data(using: .ascii) else {
			throw PrivateKeyError.invalidPrefix
		}

		guard let messageData = message.data(using: .utf8) else {
			throw SignatureError.invalidMessage
		}

		data.append(messageData)

		return data
	}

	static func ethHash(_ message: String) throws -> Data {
		let data = try ethPersonalMessage(message)

		return Util.keccak256(data)
	}

	static func createIdentityText(key: Data) -> String {
		return (
			"XMTP : Create Identity\n" +
				"\(key.toHex)\n" +
				"\n" +
				"For more info: https://xmtp.org/signatures/"
		)
	}

	static func enableIdentityText(key: Data) -> String {
		return (
			"XMTP : Enable Identity\n" +
				"\(key.toHex)\n" +
				"\n" +
				"For more info: https://xmtp.org/signatures/"
		)
	}

	init(bytes: Data, recovery: Int) {
		self.init()
		ecdsaCompact.bytes = bytes
		ecdsaCompact.recovery = UInt32(recovery)
	}

	var rawData: Data {
		switch union {
		case let .ecdsaCompact(ecdsa):
			return ecdsa.bytes + [UInt8(Int(ecdsa.recovery))]
		case let .walletEcdsaCompact(ecdsa):
			return ecdsa.bytes + [UInt8(Int(ecdsa.recovery))]
		case .none:
			return Data()
		}
	}

	mutating func ensureWalletSignature() {
		switch union {
		case let .ecdsaCompact(ecdsa):
			var walletEcdsa = Signature.WalletECDSACompact()
			walletEcdsa.bytes = ecdsa.bytes
			walletEcdsa.recovery = ecdsa.recovery
			walletEcdsaCompact = walletEcdsa
			union = .walletEcdsaCompact(walletEcdsa)
		case .walletEcdsaCompact(_), .none:
			return
		}
	}

	func verify(signedBy: PublicKey, digest: Data) throws -> Bool {
		let recoverySignature = try secp256k1.Recovery.ECDSASignature(compactRepresentation: ecdsaCompact.bytes, recoveryId: Int32(ecdsaCompact.recovery))
		let ecdsaSignature = try recoverySignature.normalize
		let signingKey = try secp256k1.Signing.PublicKey(rawRepresentation: signedBy.secp256K1Uncompressed.bytes, format: .uncompressed)

		return signingKey.ecdsa.isValidSignature(ecdsaSignature, for: digest)
	}

	func verify(signedBy: PublicKey, digest: any Digest) throws -> Bool {
		let recoverySignature = try secp256k1.Recovery.ECDSASignature(compactRepresentation: ecdsaCompact.bytes, recoveryId: Int32(ecdsaCompact.recovery))
		let ecdsaSignature = try recoverySignature.normalize
		let signingKey = try secp256k1.Signing.PublicKey(rawRepresentation: signedBy.secp256K1Uncompressed.bytes, format: .uncompressed)

		return signingKey.ecdsa.isValidSignature(ecdsaSignature, for: digest)
	}
}
