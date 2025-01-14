//
//  GroupMembershipChanged.swift
//  
//
//  Created by Pat Nakajima on 2/1/24.
//

import Foundation
import LibXMTP

public typealias GroupUpdated = Xmtp_Mls_MessageContents_GroupUpdated

public let ContentTypeGroupUpdated = ContentTypeID(authorityID: "xmtp.org", typeID: "group_updated", versionMajor: 1, versionMinor: 0)

public struct GroupUpdatedCodec: ContentCodec {
	public typealias T = GroupUpdated

	public init() {	}

	public var contentType = ContentTypeGroupUpdated

	public func encode(content: GroupUpdated) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeGroupUpdated
		encodedContent.content = try content.serializedData()

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> GroupUpdated {
		return try GroupUpdated(serializedData: content.content)
	}

	public func fallback(content: GroupUpdated) throws -> String? {
		return nil
	}

	public func shouldPush(content: GroupUpdated) throws -> Bool {
		false
	}
}
