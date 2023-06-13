//
//  GroupChatMemberAddedCodec.swift
//
//
//  Created by Pat Nakajima on 6/11/23.
//

import Foundation

public let ContentTypeGroupChatMemberAdded = ContentTypeID(authorityID: "xmtp.org", typeID: "groupChatMemberAdded", versionMajor: 1, versionMinor: 0)

public struct GroupChatMemberAdded: Codable {
	// The address of the member being added
	public var member: String
}

public struct GroupChatMemberAddedCodec: ContentCodec {
	public var contentType = ContentTypeGroupChatMemberAdded

	public func encode(content: GroupChatMemberAdded) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeGroupChatMemberAdded
		encodedContent.content = try JSONEncoder().encode(content)

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> GroupChatMemberAdded {
		let memberAdded = try JSONDecoder().decode(GroupChatMemberAdded.self, from: content.content)
		return memberAdded
	}
}
