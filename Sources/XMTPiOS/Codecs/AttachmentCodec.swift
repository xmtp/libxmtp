//
//  AttachmentCodec.swift
//
//
//  Created by Pat on 2/14/23.
//
import Foundation

public let ContentTypeAttachment = ContentTypeID(authorityID: "xmtp.org", typeID: "attachment", versionMajor: 1, versionMinor: 0)

public enum AttachmentCodecError: Error {
	case invalidParameters, unknownDecodingError
}

public struct Attachment: Codable {
	public var filename: String
	public var mimeType: String
	public var data: Data

	public init(filename: String, mimeType: String, data: Data) {
		self.filename = filename
		self.mimeType = mimeType
		self.data = data
	}
}

public struct AttachmentCodec: ContentCodec {
	public typealias T = Attachment

	public init() {}

	public var contentType = ContentTypeAttachment

	public func encode(content: Attachment) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeAttachment
		encodedContent.parameters = [
			"filename": content.filename,
			"mimeType": content.mimeType,
		]
		encodedContent.content = content.data

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> Attachment {
		guard let mimeType = content.parameters["mimeType"],
		      let filename = content.parameters["filename"]
		else {
			throw AttachmentCodecError.invalidParameters
		}

		let attachment = Attachment(filename: filename, mimeType: mimeType, data: content.content)

		return attachment
	}

    public func fallback(content: Attachment) throws -> String? {
        return "Can’t display “\(content.filename)”. This app doesn’t support attachments."
    }

	public func shouldPush(content: Attachment) throws -> Bool {
		return true
	}
}
