import Foundation

public let ContentTypeDeleteMessageRequest = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "deleteMessage",
	versionMajor: 1,
	versionMinor: 0,
)

/// Represents a request to delete a message.
/// This content type is used to request deletion of a specific message in a conversation.
/// The message will be deleted for all participants.
public struct DeleteMessageRequest: Codable, Equatable {
	/// The ID of the message to delete
	public var messageId: String

	public init(messageId: String) {
		self.messageId = messageId
	}
}

public struct DeleteMessageCodec: ContentCodec {
	public typealias T = DeleteMessageRequest

	public init() {}

	public var contentType: ContentTypeID = ContentTypeDeleteMessageRequest

	public func encode(content: DeleteMessageRequest) throws -> EncodedContent {
		let ffi = FfiDeleteMessage(
			messageId: content.messageId,
		)
		return try EncodedContent(serializedBytes: encodeDeleteMessage(request: ffi))
	}

	public func decode(content: EncodedContent) throws -> DeleteMessageRequest {
		let decoded = try decodeDeleteMessage(bytes: content.serializedBytes())
		return DeleteMessageRequest(
			messageId: decoded.messageId,
		)
	}

	public func fallback(content _: DeleteMessageRequest) throws -> String? {
		nil
	}

	public func shouldPush(content _: DeleteMessageRequest) throws -> Bool {
		false
	}
}
