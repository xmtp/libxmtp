//
//  TextCodec.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation

public let ContentTypeText = ContentTypeID(authorityID: "xmtp.org", typeID: "text", versionMajor: 1, versionMinor: 0)

enum TextCodecError: Error {
	case invalidEncoding, unknownDecodingError
}

public struct TextCodec: ContentCodec {

	public typealias T = String

	public init() {	}

	public var contentType = ContentTypeText

	public func encode(content: String) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeText
		encodedContent.parameters = ["encoding": "UTF-8"]
		encodedContent.content = Data(content.utf8)

		return encodedContent
	}

	public func decode(content: EncodedContent) throws -> String {
		if let encoding = content.parameters["encoding"], encoding != "UTF-8" {
			throw TextCodecError.invalidEncoding
		}

		if let contentString = String(data: content.content, encoding: .utf8) {
			return contentString
		} else {
			throw TextCodecError.unknownDecodingError
		}
	}

    public func fallback(content: String) throws -> String? {
        return nil
    }

	public func shouldPush(content: String) throws -> Bool {
		return true
	}
}
