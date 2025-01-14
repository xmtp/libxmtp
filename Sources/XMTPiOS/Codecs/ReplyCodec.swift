//
//  ReplyCodec.swift
//
//
//  Created by Naomi Plasterer on 7/26/23.
//

import Foundation

public let ContentTypeReply = ContentTypeID(authorityID: "xmtp.org", typeID: "reply", versionMajor: 1, versionMinor: 0)

public struct Reply {
	public var reference: String
	public var content: Any
	public var contentType: ContentTypeID

	public init(reference: String, content: Any, contentType: ContentTypeID) {
		self.reference = reference
		self.content = content
		self.contentType = contentType
	}
}

public struct ReplyCodec: ContentCodec {
	public var contentType = ContentTypeReply

	public init() {}

	public func encode(content reply: Reply) throws -> EncodedContent {
		var encodedContent = EncodedContent()
		let replyCodec = Client.codecRegistry.find(for: reply.contentType)

		encodedContent.type = contentType
		// TODO: cut when we're certain no one is looking for "contentType" here.
		encodedContent.parameters["contentType"] = reply.contentType.description
		encodedContent.parameters["reference"] = reply.reference
		encodedContent.content = try encodeReply(codec: replyCodec, content: reply.content).serializedData()

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> Reply {
		guard let reference = content.parameters["reference"] else {
			throw CodecError.invalidContent
		}

		let replyEncodedContent = try EncodedContent(serializedData: content.content)
		let replyCodec = Client.codecRegistry.find(for: replyEncodedContent.type)
		let replyContent = try replyCodec.decode(content: replyEncodedContent)

		return Reply(
			reference: reference,
			content: replyContent,
			contentType: replyCodec.contentType
		)
	}

	func encodeReply<Codec: ContentCodec>(codec: Codec, content: Any) throws -> EncodedContent {
		if let content = content as? Codec.T {
			return try codec.encode(content: content)
		} else {
			throw CodecError.invalidContent
		}
	}

    public func fallback(content: Reply) throws -> String? {
        return "Replied with “\(content.content)” to an earlier message"
    }

	public func shouldPush(content: Reply) throws -> Bool {
		return true
	}
}
