import Foundation

public enum SignerType {
	case EOA, SCW
}

/// A standardized signing interface for XMTP clients supporting EOA, SCW, and Passkeys.
public protocol SigningKey {
	/// The identity associated with the signing key (e.g., Ethereum address or Passkey identifier).
	var identity: PublicIdentity { get }

	/// The signer type (default: EOA).
	var type: SignerType { get }

	/// The blockchain chain ID (used for SCW, nil for others).
	var chainId: Int64? { get }

	/// The block number for verification (optional).
	var blockNumber: Int64? { get }

	/// Sign a message and return a `SignedData` structure containing the signature and metadata.
	func sign(_ message: String) async throws -> SignedData
}

/// Default implementations for properties
public extension SigningKey {
	var type: SignerType {
		.EOA
	}

	var chainId: Int64? {
		nil
	}

	var blockNumber: Int64? {
		nil
	}
}
