import Foundation

/// Represents a secp256k1 private key.  ``PrivateKey`` conforms to ``SigningKey`` so you can use it
/// to create a ``Client``.
public typealias PrivateKey = Xmtp_MessageContents_PrivateKey
typealias PublicKey = Xmtp_MessageContents_PublicKey

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
		PublicIdentity(kind: .ethereum, identifier: walletAddress)
	}

	public func sign(_ message: String) async throws -> SignedData {
		let digest = try KeyUtilx.ethHash(message)
		let signatureData = try KeyUtilx.sign(message: digest, with: secp256K1.bytes, hashing: false)

		guard signatureData.count == 65 else {
			throw PrivateKeyError.invalidSignature
		}

		return SignedData(
			rawData: signatureData,
			publicKey: publicKey.secp256K1Uncompressed.bytes,
			authenticatorData: nil,
			clientDataJson: nil
		)
	}
}

public extension PrivateKey {
	/// **Generate a new private key like in Kotlin**
	static func generate() throws -> PrivateKey {
		let privateKeyData = try Data(Crypto.secureRandomBytes(count: 32))
		return try PrivateKey(privateKeyData)
	}

	/// **Initialize from raw private key data**
	init(_ privateKeyData: Data) throws {
		self.init()
		timestamp = UInt64(Date().timeIntervalSince1970 * 1000) // Match Kotlin's timestamp
		secp256K1.bytes = privateKeyData

		let publicData = try KeyUtilx.generatePublicKey(from: privateKeyData)
		publicKey.secp256K1Uncompressed.bytes = publicData
		publicKey.timestamp = timestamp

		// Validate that we can generate a wallet address - throw if invalid
		_ = try KeyUtilx.generateAddress(from: publicData)
	}

	/// **Compute Ethereum wallet address from public key (matching Kotlin)**
	internal var walletAddress: String {
		publicKey.walletAddress
	}
}

/// **Compute wallet address from PublicKey like in Kotlin**
extension PublicKey {
	var walletAddress: String {
		// Safe to force unwrap since we validate address generation during PrivateKey initialization
		// swiftlint:disable:next force_try
		try! KeyUtilx.generateAddress(from: secp256K1Uncompressed.bytes).lowercased()
	}
}
