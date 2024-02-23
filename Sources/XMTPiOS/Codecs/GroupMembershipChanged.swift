//
//  GroupMembershipChanged.swift
//  
//
//  Created by Pat Nakajima on 2/1/24.
//

import Foundation
import LibXMTP

public typealias GroupMembershipChanges = Xmtp_Mls_MessageContents_GroupMembershipChanges

public let ContentTypeGroupMembershipChanged = ContentTypeID(authorityID: "xmtp.org", typeID: "group_membership_change", versionMajor: 1, versionMinor: 0)

public struct GroupMembershipChangedCodec: ContentCodec {
	public typealias T = GroupMembershipChanges

	public init() {	}

	public var contentType = ContentTypeGroupMembershipChanged

	public func encode(content: GroupMembershipChanges, client _: Client) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeGroupMembershipChanged
		encodedContent.content = try content.serializedData()

		return encodedContent
	}

	public func decode(content: EncodedContent, client _: Client) throws -> GroupMembershipChanges {
		return try GroupMembershipChanges(serializedData: content.content)
	}

	public func fallback(content: GroupMembershipChanges) throws -> String? {
		return nil
	}

	public func shouldPush(content: GroupMembershipChanges) throws -> Bool {
		false
	}
}
