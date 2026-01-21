import Foundation

public let ContentTypeDeleteMessage = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "deleteMessage",
	versionMajor: 1,
	versionMinor: 0
)

/// Represents a delete message request sent when a user wants to delete a message.
/// This content type is used to request deletion of a specific message in a conversation.
///
/// - Note: Delete message requests are automatically sent when calling `deleteMessage()` on a conversation.
///   You should not need to manually encode or send this content type.
public struct DeleteMessageRequest: Codable, Equatable {
	/// The ID of the message to delete (hex-encoded).
	public var messageId: String

	public init(messageId: String) {
		self.messageId = messageId
	}
}

public struct DeleteMessageCodec: ContentCodec {
	public typealias T = DeleteMessageRequest

	public init() {}

	public var contentType: ContentTypeID = ContentTypeDeleteMessage

	public func encode(content: DeleteMessageRequest) throws -> EncodedContent {
		var proto = Xmtp_Mls_MessageContents_ContentTypes_DeleteMessage()
		proto.messageID = content.messageId
		var encoded = EncodedContent()
		encoded.type = contentType
		encoded.content = try proto.serializedData()
		return encoded
	}

	public func decode(content: EncodedContent) throws -> DeleteMessageRequest {
		let proto = try Xmtp_Mls_MessageContents_ContentTypes_DeleteMessage(serializedBytes: content.content)
		return DeleteMessageRequest(messageId: proto.messageID)
	}

	public func fallback(content _: DeleteMessageRequest) throws -> String? {
		nil
	}

	public func shouldPush(content _: DeleteMessageRequest) throws -> Bool {
		false
	}
}
