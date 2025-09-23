//
//  InboxState.swift
//
//
//  Created by Naomi Plasterer on 8/21/24.
//

import Foundation

public struct InboxState {
	var ffiInboxState: FfiInboxState
	
	init(ffiInboxState: FfiInboxState) {
		self.ffiInboxState = ffiInboxState
	}

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

}
