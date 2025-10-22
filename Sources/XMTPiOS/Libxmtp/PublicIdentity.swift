//
//  PublicIdentity.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 3/6/25.
//

import Foundation

public enum IdentityKind {
	case ethereum
	case passkey
}

public struct PublicIdentity {
	let ffiPrivate: FfiIdentifier

	public init(kind: IdentityKind, identifier: String) {
		ffiPrivate = FfiIdentifier(
			identifier: identifier,
			identifierKind: kind.toFfiPublicIdentifierKind()
		)
	}

	init(ffiPrivate: FfiIdentifier) {
		self.ffiPrivate = ffiPrivate
	}

	public var kind: IdentityKind {
		ffiPrivate.identifierKind.toIdentityKind()
	}

	public var identifier: String {
		ffiPrivate.identifier.lowercased()
	}
}

public extension IdentityKind {
	func toFfiPublicIdentifierKind() -> FfiIdentifierKind {
		switch self {
		case .ethereum:
			return .ethereum
		case .passkey:
			return .passkey
		}
	}
}

public extension FfiIdentifierKind {
	func toIdentityKind() -> IdentityKind {
		switch self {
		case .ethereum:
			return .ethereum
		case .passkey:
			return .passkey
		}
	}
}
