//
//  CodecRegistry.swift
//
//
//  Created by Pat Nakajima on 12/22/22.
//

import Foundation

struct CodecRegistry {
	var codecs: [String: any ContentCodec] = [:]

	mutating func register(codec: any ContentCodec) {
		codecs[codec.id] = codec
	}

	func find(for contentType: ContentTypeID?) -> any ContentCodec {
		guard let contentType else {
			return TextCodec()
		}

		if let codec = codecs[contentType.id] {
			return codec
		}

		return TextCodec()
	}
}
