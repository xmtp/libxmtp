//
//  GroupUpdatedCodec.swift
//
//
//  Created by Pat Nakajima on 2/1/24.
//

import Foundation

public typealias GroupUpdated = Xmtp_Mls_MessageContents_GroupUpdated

public let ContentTypeGroupUpdated = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "group_updated",
	versionMajor: 1,
	versionMinor: 0,
)

public struct GroupUpdatedCodec: ContentCodec {
	public typealias T = GroupUpdated

	public init() {}

	public var contentType = ContentTypeGroupUpdated

	public func encode(content: GroupUpdated) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeGroupUpdated
		encodedContent.content = try content.serializedBytes()

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> GroupUpdated {
		try GroupUpdated(serializedData: content.content)
	}

	public func fallback(content _: GroupUpdated) throws -> String? {
		nil
	}

	public func shouldPush(content _: GroupUpdated) throws -> Bool {
		false
	}
}
