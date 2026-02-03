//
//  GroupMembershipResult.swift
//
//
//  Created by Naomi Plasterer on 3/10/25.
//

import Foundation

public struct GroupMembershipResult {
	var ffiGroupMembershipResult: FfiUpdateGroupMembershipResult

	public var addedMembers: [InboxId] {
		ffiGroupMembershipResult.addedMembers.map(\.key)
	}

	public var removedMembers: [InboxId] {
		ffiGroupMembershipResult.removedMembers
	}

	public var failedInstallationIds: [String] {
		ffiGroupMembershipResult.failedInstallations.map(\.toHex)
	}
}
