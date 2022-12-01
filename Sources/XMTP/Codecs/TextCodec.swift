//
//  TextCodec.swift
//
//
//  Created by Pat Nakajima on 11/28/22.
//

import Foundation
import XMTPProto

let ContentTypeText = ContentTypeID(authorityID: "xmtp.org", typeID: "text", versionMajor: 1, versionMinor: 0)

enum TextCodecError: Error {
	case invalidEncoding, unknownDecodingError
}

struct TextCodec: ContentCodec {
	typealias T = String

	var contentType = ContentTypeText

	func encode(content: String) throws -> EncodedContent {
		var encodedContent = EncodedContent()

		encodedContent.type = ContentTypeText
		encodedContent.parameters = ["encoding": "UTF-8"]
		encodedContent.content = Data(content.utf8)

		return encodedContent
	}

	func decode(content: EncodedContent) throws -> String {
		if let encoding = content.parameters["encoding"], encoding != "UTF-8" {
			throw TextCodecError.invalidEncoding
		}

		if let contentString = String(data: content.content, encoding: .utf8) {
			return contentString
		} else {
			throw TextCodecError.unknownDecodingError
		}
	}
}
