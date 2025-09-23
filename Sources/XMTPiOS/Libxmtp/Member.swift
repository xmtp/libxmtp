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
    
    init(ffiGroupMember: FfiConversationMember) {
        self.ffiGroupMember = ffiGroupMember
    }

    public var inboxId: InboxId {
        ffiGroupMember.inboxId
    }
    
    public var identities: [PublicIdentity] {
		ffiGroupMember.accountIdentifiers.map { PublicIdentity(ffiPrivate: $0) }
    }

	public var permissionLevel: PermissionLevel {
        switch ffiGroupMember.permissionLevel {
        case .member:
            return PermissionLevel.Member
        case .admin:
            return PermissionLevel.Admin
        case .superAdmin:
            return PermissionLevel.SuperAdmin
        }
	}
	
	public var consentState: ConsentState {
		ffiGroupMember.consentState.fromFFI
	}
}

