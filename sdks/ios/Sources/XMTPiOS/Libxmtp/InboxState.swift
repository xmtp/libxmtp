//
//  InboxState.swift
//
//
//  Created by Naomi Plasterer on 8/21/24.
//

import Foundation

public typealias SignatureKind = FfiSignatureKind

public struct InboxState {
	var ffiInboxState: FfiInboxState

	public var inboxId: InboxId {
		ffiInboxState.inboxId
	}

	public var identities: [PublicIdentity] {
		ffiInboxState.accountIdentities.map { PublicIdentity(ffiPrivate: $0) }
	}

	public var installations: [Installation] {
		ffiInboxState.installations.map { Installation(ffiInstallation: $0) }
	}

	public var recoveryIdentity: PublicIdentity {
		PublicIdentity(ffiPrivate: ffiInboxState.recoveryIdentity)
	}

	/// The type of signature that was used to create the inbox initially.
	/// Future signatures from this identity must be of the same kind
	public var creationSignatureKind: SignatureKind? {
		ffiInboxState.creationSignatureKind
	}
}
