//
//  PublicIdentity.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 3/6/25.
//

import Foundation

/// The type of account identifier used to create an XMTP identity.
public enum IdentityKind {
	/// An Ethereum wallet address (EOA or smart contract wallet).
	case ethereum
	/// A Passkey-based identifier.
	case passkey
}

/// Represents a public identity on the XMTP network.
///
/// A `PublicIdentity` ties an external account identifier (such as an Ethereum address
/// or a Passkey) to its XMTP inbox. Use it when creating clients, looking up
/// inbox IDs, or adding/removing accounts.
///
/// ```swift
/// let identity = PublicIdentity(
///     kind: .ethereum,
///     identifier: "0xAbC123..."
/// )
/// let canMessage = try await client.canMessage(identity: identity)
/// ```
public struct PublicIdentity {
	let ffiPrivate: FfiIdentifier

	/// Creates a public identity from an account kind and its identifier string.
	///
	/// - Parameters:
	///   - kind: The type of account (`.ethereum` or `.passkey`).
	///   - identifier: The account identifier (e.g., an Ethereum address).
	public init(kind: IdentityKind, identifier: String) {
		ffiPrivate = FfiIdentifier(
			identifier: identifier,
			identifierKind: kind.toFfiPublicIdentifierKind()
		)
	}

	init(ffiPrivate: FfiIdentifier) {
		self.ffiPrivate = ffiPrivate
	}

	/// The type of account this identity represents.
	public var kind: IdentityKind {
		ffiPrivate.identifierKind.toIdentityKind()
	}

	/// The lowercased account identifier string (e.g., an Ethereum address).
	public var identifier: String {
		ffiPrivate.identifier.lowercased()
	}
}

public extension IdentityKind {
	/// Converts this identity kind to its FFI representation.
	func toFfiPublicIdentifierKind() -> FfiIdentifierKind {
		switch self {
		case .ethereum:
			.ethereum
		case .passkey:
			.passkey
		}
	}
}

public extension FfiIdentifierKind {
	/// Converts this FFI identifier kind to the Swift ``IdentityKind`` type.
	func toIdentityKind() -> IdentityKind {
		switch self {
		case .ethereum:
			.ethereum
		case .passkey:
			.passkey
		}
	}
}
