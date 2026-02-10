//
//  Member.swift
//
//
//  Created by Naomi Plasterer on 5/27/24.
//

import Foundation

public enum PermissionLevel {
	case Member, Admin, SuperAdmin
}

public struct Member {
	var ffiGroupMember: FfiConversationMember

	public var inboxId: InboxId {
		ffiGroupMember.inboxId
	}

	public var identities: [PublicIdentity] {
		ffiGroupMember.accountIdentifiers.map { PublicIdentity(ffiPrivate: $0) }
	}

	public var permissionLevel: PermissionLevel {
		switch ffiGroupMember.permissionLevel {
		case .member:
			PermissionLevel.Member
		case .admin:
			PermissionLevel.Admin
		case .superAdmin:
			PermissionLevel.SuperAdmin
		}
	}

	public var consentState: ConsentState {
		ffiGroupMember.consentState.fromFFI
	}
}
