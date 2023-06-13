//
//  GroupChatTitleChangedCodec.swift
//
//
//  Created by Pat Nakajima on 6/11/23.
//

import Foundation

public let ContentTypeGroupTitleChangedAdded = ContentTypeID(authorityID: "xmtp.org", typeID: "groupChatTitleChanged", versionMajor: 1, versionMinor: 0)

public struct GroupChatTitleChanged: Codable {
	// The new title
	public var newTitle: String
}

public struct GroupChatTitleChangedCodec: ContentCodec {
	public var contentType = ContentTypeGroupTitleChangedAdded

	public func encode(content: GroupChatTitleChanged) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeGroupTitleChangedAdded
		encodedContent.content = try JSONEncoder().encode(content)

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> GroupChatTitleChanged {
		let titleChanged = try JSONDecoder().decode(GroupChatTitleChanged.self, from: content.content)
		return titleChanged
	}
}
