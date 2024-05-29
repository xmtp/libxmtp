//
//  Member.swift
//
//
//  Created by Naomi Plasterer on 5/27/24.
//

import Foundation
import LibXMTP

public enum PermissionLevel {
    case Member, Admin, SuperAdmin
}

public struct Member {
    var ffiGroupMember: FfiGroupMember
    
    init(ffiGroupMember: FfiGroupMember) {
        self.ffiGroupMember = ffiGroupMember
    }

    public var inboxId: String {
        ffiGroupMember.inboxId
    }
    
    public var addresses: [String] {
        ffiGroupMember.accountAddresses
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
}

