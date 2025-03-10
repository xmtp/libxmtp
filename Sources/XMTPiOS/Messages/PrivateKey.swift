import Foundation
import LibXMTP

/// Represents a secp256k1 private key.  ``PrivateKey`` conforms to ``SigningKey`` so you can use it
/// to create a ``Client``.
public typealias PrivateKey = Xmtp_MessageContents_PrivateKey

enum PrivateKeyError: Error, CustomStringConvertible {
	case invalidSignatureText, invalidPrefix, invalidSignature

	var description: String {
		switch self {
		case .invalidSignatureText:
			return "PrivateKeyError.invalidSignatureText"
		case .invalidPrefix:
			return "PrivateKeyError.invalidPrefix"
		case .invalidSignature:
			return "PrivateKeyError.invalidSignature"
		}
	}
}

extension PrivateKey: SigningKey {
	public var identity: PublicIdentity {
		return PublicIdentity(kind: .ethereum, identifier: walletAddress)
	}

	public func sign(_ data: Data) async throws -> Signature {
		let signatureData = try KeyUtilx.sign(
			message: data, with: secp256K1.bytes, hashing: false)
		var signature = Signature()

		signature.ecdsaCompact.bytes = signatureData[0..<64]
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

		let publicData = try KeyUtilx.generatePublicKey(from: privateKeyData)
		publicKey.secp256K1Uncompressed.bytes = publicData
		publicKey.timestamp = timestamp
	}

	public static func generate() throws -> PrivateKey {
		let data = Data(try Crypto.secureRandomBytes(count: 32))
		return try PrivateKey(data)
	}

	internal var walletAddress: String {
		publicKey.walletAddress
	}
}
