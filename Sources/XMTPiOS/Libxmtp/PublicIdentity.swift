//
//  PublicIdentity.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 3/6/25.
//

import Foundation
import LibXMTP

public enum IdentityKind {
	case ethereum
	case passkey
}

public struct PublicIdentity {
	let ffiPrivate: FfiIdentifier

	public init(kind: IdentityKind, identifier: String) {
		self.ffiPrivate = FfiIdentifier(
			identifier: identifier,
			identifierKind: kind.toFfiPublicIdentifierKind()
		)
	}

	init(ffiPrivate: FfiIdentifier) {
		self.ffiPrivate = ffiPrivate
	}

	public var kind: IdentityKind {
		return ffiPrivate.identifierKind.toIdentityKind()
	}

	public var identifier: String {
		return ffiPrivate.identifier.lowercased()
	}
}

extension IdentityKind {
	public func toFfiPublicIdentifierKind() -> FfiIdentifierKind {
		switch self {
		case .ethereum:
			return .ethereum
		case .passkey:
			return .passkey
		}
	}
}

extension FfiIdentifierKind {
	public func toIdentityKind() -> IdentityKind {
		switch self {
		case .ethereum:
			return .ethereum
		case .passkey:
			return .passkey
		}
	}
}
