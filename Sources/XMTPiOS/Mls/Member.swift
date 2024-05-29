//
//  Member.swift
//
//
//  Created by Naomi Plasterer on 5/27/24.
//

import Foundation
import LibXMTP

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
}

