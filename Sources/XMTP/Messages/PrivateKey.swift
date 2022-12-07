//
//  PrivateKey.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import XMTPProto

public typealias PrivateKey = Xmtp_MessageContents_PrivateKey

enum PrivateKeyError: Error {
	case invalidSignatureText, invalidPrefix, invalidSignature
}

extension PrivateKey: SigningKey {
	public var address: String {
		walletAddress
	}

	func matches(_ publicKey: PublicKey) -> Bool {
		do {
			return try self.publicKey.recoverKeySignedPublicKey() == (try publicKey.recoverKeySignedPublicKey())
		} catch {
			return false
		}
	}

	public func sign(_ data: Data) async throws -> Signature {
		let signatureData = try KeyUtil.sign(message: data, with: secp256K1.bytes, hashing: false)
		var signature = Signature()

		signature.ecdsaCompact.bytes = signatureData[0 ..< 64]
		signature.ecdsaCompact.recovery = UInt32(signatureData[64])

		return signature
	}

	public func sign(message: String) async throws -> Signature {
		let digest = try Signature.ethHash(message)

		return try await sign(digest)
	}
}

extension PrivateKey {
	// Easier conversion from the secp256k1 library's Private keys to our proto type.
	public init(_ privateKeyData: Data) throws {
		self.init()
		timestamp = UInt64(Date().millisecondsSinceEpoch)
		secp256K1.bytes = privateKeyData

		let publicData = try KeyUtil.generatePublicKey(from: privateKeyData)
		publicKey.secp256K1Uncompressed.bytes = publicData
		publicKey.timestamp = timestamp
	}

	public static func generate() throws -> PrivateKey {
		let data = Data(try Crypto.secureRandomBytes(count: 32))
		return try PrivateKey(data)
	}

	var walletAddress: String {
		publicKey.walletAddress
	}

	func sign(key: UnsignedPublicKey) async throws -> SignedPublicKey {
		let bytes = try key.serializedData()
		let digest = SHA256.hash(data: bytes)
		let signature = try await sign(Data(digest.bytes))

		var signedPublicKey = SignedPublicKey()
		signedPublicKey.signature = signature
		signedPublicKey.keyBytes = bytes

		return signedPublicKey
	}
}
