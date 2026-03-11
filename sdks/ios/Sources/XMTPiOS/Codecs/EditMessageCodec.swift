import Foundation

public let ContentTypeEditMessageRequest = ContentTypeID(
	authorityID: "xmtp.org",
	typeID: "editMessage",
	versionMajor: 1,
	versionMinor: 0
)

/// Represents a request to edit a message.
/// This content type is used to request an edit of a specific message in a conversation.
/// Only the original sender can edit their own messages.
public struct EditMessageRequest: Codable, Equatable {
	public var messageId: String
	public var editedContent: EncodedContent?

	public init(messageId: String, editedContent: EncodedContent? = nil) {
		self.messageId = messageId
		self.editedContent = editedContent
	}
}

public struct EditMessageCodec: ContentCodec {
	public typealias T = EditMessageRequest

	public init() {}

	public var contentType: ContentTypeID = ContentTypeEditMessageRequest

	public func encode(content: EditMessageRequest) throws -> EncodedContent {
		let ffiEditedContent: FfiEncodedContent? = try content.editedContent.map { encoded in
			FfiEncodedContent(
				typeId: encoded.hasType
					? FfiContentTypeId(
						authorityId: encoded.type.authorityID,
						typeId: encoded.type.typeID,
						versionMajor: UInt32(encoded.type.versionMajor),
						versionMinor: UInt32(encoded.type.versionMinor)
					)
					: nil,
				parameters: encoded.parameters,
				fallback: encoded.hasFallback ? encoded.fallback : nil,
				compression: encoded.hasCompression ? Int32(encoded.compression.rawValue) : nil,
				content: encoded.content
			)
		}

		let ffi = FfiEditMessage(
			messageId: content.messageId,
			editedContent: ffiEditedContent
		)
		return try EncodedContent(serializedBytes: encodeEditMessage(request: ffi))
	}

	public func decode(content: EncodedContent) throws -> EditMessageRequest {
		let decoded = try decodeEditMessage(bytes: content.serializedBytes())
		return EditMessageRequest(
			messageId: decoded.messageId,
			editedContent: decoded.editedContent.map { ffiContent in
				var encoded = EncodedContent()
				if let type = ffiContent.typeId {
					var contentType = ContentTypeID()
					contentType.authorityID = type.authorityId
					contentType.typeID = type.typeId
					contentType.versionMajor = UInt32(type.versionMajor)
					contentType.versionMinor = UInt32(type.versionMinor)
					encoded.type = contentType
				}
				encoded.parameters = ffiContent.parameters
				if let fallback = ffiContent.fallback {
					encoded.fallback = fallback
				}
				encoded.content = Data(ffiContent.content)
				return encoded
			}
		)
	}

	public func fallback(content _: EditMessageRequest) throws -> String? {
		nil
	}

	public func shouldPush(content _: EditMessageRequest) throws -> Bool {
		false
	}
}
